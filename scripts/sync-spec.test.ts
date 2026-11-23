import { spawnSync } from 'node:child_process'
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'bun:test'

type CommandResult = {
  exitCode: number
  stdout: string
  stderr: string
}

type IsolatedProject = {
  root: string
  scriptPath: string
  collectionPath: string
}

const sourceScriptPath = fileURLToPath(new URL('./sync-spec.ts', import.meta.url))

const collectionFixture = {
  info: {
    name: 'Sanitized SIMASTER fixture',
    description: 'Static APK splits analyzed on 2031-02-03.',
  },
  variable: [{ key: 'api_base', value: 'https://api.simaster.ugm.ac.id' }],
  item: [
    {
      name: '01 - OAuth',
      item: [
        {
          name: 'Authorize',
          request: {
            method: 'GET',
            url: {
              raw: 'https://oauth.simaster.ugm.ac.id/oauth/authorize?response_type=code&client_id=fixture-client-secret&redirect_uri=http%3A%2F%2F127.0.0.1%3A53682%2Fcallback&scope=openid%20user.read&state=fixture-state&nonce=fixture-nonce&code_challenge=fixture-challenge&code_challenge_method=S256',
            },
          },
        },
        {
          name: 'Exchange code',
          request: {
            method: 'POST',
            url: 'https://oauth.simaster.ugm.ac.id/oauth/token',
            body: {
              mode: 'urlencoded',
              urlencoded: [
                { key: 'grant_type', value: 'authorization_code' },
                { key: 'code', value: 'fixture-authorization-code' },
                { key: 'redirect_uri', value: 'http://127.0.0.1:53682/callback' },
                { key: 'client_id', value: 'fixture-client-secret' },
                { key: 'code_verifier', value: 'fixture-pkce-verifier' },
              ],
            },
          },
        },
      ],
    },
    {
      name: 'API',
      item: [
        {
          name: 'Read example',
          request: {
            method: 'POST',
            url: { raw: '{{api_base}}/v1/example?access_token=fixture-query-token' },
            body: { mode: 'raw', raw: '{"password":"fixture-password","message":"safe sample"}' },
          },
          response: [
            {
              name: 'Example response',
              code: 200,
              body: '{"access_token":"fixture-response-token","message":"safe sample"}',
            },
          ],
        },
      ],
    },
  ],
}

async function createIsolatedProject(): Promise<IsolatedProject> {
  const root = await mkdtemp(join(tmpdir(), 'simaster-sync-spec-'))
  const scriptsPath = join(root, 'scripts')
  const scriptPath = join(scriptsPath, 'sync-spec.ts')
  const collectionPath = join(root, 'SIMASTER.postman_collection.json')
  await mkdir(scriptsPath, { recursive: true })
  await copyFile(sourceScriptPath, scriptPath)
  await writeFile(collectionPath, JSON.stringify(collectionFixture))
  return { root, scriptPath, collectionPath }
}

function execute(scriptPath: string, args: string[]): CommandResult {
  const result = spawnSync(process.execPath, [scriptPath, ...args], { encoding: 'utf8' })
  return {
    exitCode: result.status ?? 1,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  }
}

function parameterBlock(document: string, name: string): string {
  const start = document.indexOf(`"name": "${name}"`)
  expect(start).toBeGreaterThanOrEqual(0)
  const next = document.indexOf('\n    -\n', start)
  return document.slice(start, next < 0 ? undefined : next)
}

test('generates accurate OAuth contracts, redacted examples, and checks drift', async () => {
  const project = await createIsolatedProject()
  try {
    const generated = execute(project.scriptPath, [project.collectionPath])
    expect(generated.exitCode).toBe(0)
    expect(generated.stdout).toContain('Generated 5 files')

    const docsRoot = join(project.root, 'docs', 'openapi', 'simaster-vnext')
    const authorize = await readFile(join(docsRoot, 'paths', 'oauth--authorize-get.yaml'), 'utf8')
    const token = await readFile(join(docsRoot, 'paths', 'oauth--token-post.yaml'), 'utf8')
    const example = await readFile(join(docsRoot, 'paths', 'v1--example-post.yaml'), 'utf8')
    const provenance = await readFile(join(project.root, 'docs', 'SIMASTER-VNEXT-API.md'), 'utf8')

    const authorizationParameters = authorize.slice(0, authorize.indexOf('"responses":'))
    for (const name of ['response_type', 'client_id', 'redirect_uri']) {
      expect(parameterBlock(authorizationParameters, name)).toContain('"required": true')
    }
    for (const name of ['scope', 'state', 'code_challenge', 'code_challenge_method']) {
      expect(parameterBlock(authorizationParameters, name)).not.toContain('"required": true')
    }
    expect(authorizationParameters).not.toContain('"name": "nonce"')
    expect(authorize).toContain('UGM does not list `nonce` as an authorization query parameter')

    const tokenRequired = token.slice(token.indexOf('"required":\n'))
    expect(tokenRequired).toContain('- "grant_type"')
    expect(tokenRequired).not.toContain('- "code"')
    expect(tokenRequired).not.toContain('- "code_verifier"')
    expect(token).toContain('required by the CLI PKCE flow')

    expect(authorize).toContain('on 2031-02-03')
    expect(token).toContain('on 2031-02-03')
    expect(provenance).toContain('on 2031-02-03')
    expect(provenance).not.toContain('on 2026-11-23')

    const allGenerated = [authorize, token, example].join('\n')
    for (const secret of [
      'fixture-client-secret',
      'fixture-authorization-code',
      'fixture-pkce-verifier',
      'fixture-query-token',
      'fixture-password',
      'fixture-response-token',
    ]) {
      expect(allGenerated).not.toContain(secret)
    }
    expect(example).toContain('<redacted>')

    const cleanCheck = execute(project.scriptPath, ['--check', project.collectionPath])
    expect(cleanCheck.exitCode).toBe(0)
    expect(cleanCheck.stdout).toContain('match the collection')

    await writeFile(join(docsRoot, 'paths', 'oauth--token-post.yaml'), `${token}\nchanged`)
    const driftCheck = execute(project.scriptPath, ['--check', project.collectionPath])
    expect(driftCheck.exitCode).toBe(1)
    expect(driftCheck.stderr).toContain('Generated files are out of date')

    const regenerated = execute(project.scriptPath, [project.collectionPath])
    expect(regenerated.exitCode).toBe(0)
    await writeFile(join(docsRoot, 'paths', 'stale.yaml'), 'stale')
    const staleCheck = execute(project.scriptPath, ['--check', project.collectionPath])
    expect(staleCheck.exitCode).toBe(1)
    expect(staleCheck.stderr).toContain('stale.yaml')

    const staleCleanup = execute(project.scriptPath, [project.collectionPath])
    expect(staleCleanup.exitCode).toBe(0)
    const finalCheck = execute(project.scriptPath, ['--check', project.collectionPath])
    expect(finalCheck.exitCode).toBe(0)
  } finally {
    await rm(project.root, { recursive: true, force: true })
  }
})

test('reports usage and input failures with a nonzero exit code', async () => {
  const project = await createIsolatedProject()
  try {
    const usage = execute(project.scriptPath, [])
    expect(usage.exitCode).toBe(1)
    expect(usage.stderr).toContain('Usage: bun scripts/sync-spec.ts [--check] <postman-collection.json>')

    const missing = execute(project.scriptPath, [join(project.root, 'missing.json')])
    expect(missing.exitCode).toBe(1)
    expect(missing.stderr).not.toBe('')

    const invalidPath = join(project.root, 'invalid.json')
    await writeFile(invalidPath, '{}')
    const invalid = execute(project.scriptPath, [invalidPath])
    expect(invalid.exitCode).toBe(1)
    expect(invalid.stderr).toContain('Postman collection.info must be an object')
  } finally {
    await rm(project.root, { recursive: true, force: true })
  }
})
