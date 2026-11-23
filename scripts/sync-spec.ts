import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { basename, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

type SpecValue = null | boolean | number | string | SpecValue[] | { [key: string]: SpecValue }

type PostmanBody =
  | { kind: 'raw'; value: string }
  | { kind: 'urlencoded'; fields: string[] }
  | { kind: 'none' }

type PostmanResponse = {
  name: string
  code?: number
  body?: string
}

type PostmanRequest = {
  method: string
  rawUrl: string
  headers: string[]
  description?: string
  body: PostmanBody
  responses: PostmanResponse[]
}

type CollectionItem =
  | { kind: 'folder'; name: string; children: CollectionItem[] }
  | { kind: 'request'; name: string; request: PostmanRequest }

type Collection = {
  name: string
  description: string
  postmanId?: string
  variables: Map<string, string>
  items: CollectionItem[]
}

type Endpoint = {
  name: string
  folder: string
  method: string
  path: string
  server: string
  queryNames: string[]
  queryExamples: { name: string; value: string }[]
  request: PostmanRequest
}

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const outputRoot = join(repositoryRoot, 'docs', 'openapi', 'simaster-vnext')
const pathsRoot = join(outputRoot, 'paths')
const humanReferencePath = join(repositoryRoot, 'docs', 'SIMASTER-VNEXT-API.md')
const excludedPageRequests = new Set([
  'SIMASTER services page',
  'SIMASTER home page',
  'Report page',
  'ULT portal',
  'RSA clinic page',
  'DTI knowledge base',
])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isSpecRecord(value: SpecValue): value is { [key: string]: SpecValue } {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function requiredString(record: Record<string, unknown>, key: string, context: string): string {
  const value = record[key]
  if (typeof value !== 'string') {
    throw new Error(`${context}.${key} must be a string`)
  }
  return value
}

function optionalString(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key]
  return typeof value === 'string' ? value : undefined
}

function parseBody(value: unknown, context: string): PostmanBody {
  if (!isRecord(value)) return { kind: 'none' }

  const mode = value.mode
  if (mode === 'raw') {
    return { kind: 'raw', value: optionalString(value, 'raw') ?? '' }
  }
  if (mode === 'urlencoded' && Array.isArray(value.urlencoded)) {
    const fields = value.urlencoded.map((field, index) => {
      if (!isRecord(field)) throw new Error(`${context}.urlencoded[${index}] must be an object`)
      return requiredString(field, 'key', `${context}.urlencoded[${index}]`)
    })
    return { kind: 'urlencoded', fields }
  }
  return { kind: 'none' }
}

function parseResponses(value: unknown, context: string): PostmanResponse[] {
  if (!Array.isArray(value)) return []
  return value.map((entry, index) => {
    if (!isRecord(entry)) throw new Error(`${context}[${index}] must be an object`)
    const code = typeof entry.code === 'number' ? entry.code : undefined
    const body = typeof entry.body === 'string' ? entry.body : undefined
    return { name: optionalString(entry, 'name') ?? 'Saved response', code, body }
  })
}

function parseItem(value: unknown, context: string): CollectionItem {
  if (!isRecord(value)) throw new Error(`${context} must be an object`)
  const name = requiredString(value, 'name', context)
  if (Array.isArray(value.item)) {
    return {
      kind: 'folder',
      name,
      children: value.item.map((child, index) => parseItem(child, `${context}.item[${index}]`)),
    }
  }

  if (!isRecord(value.request)) throw new Error(`${context}.request must be an object`)
  const request = value.request
  const url = request.url
  const rawUrl = typeof url === 'string'
    ? url
    : isRecord(url)
      ? optionalString(url, 'raw')
      : undefined
  if (!rawUrl) throw new Error(`${context}.request.url.raw must be a non-empty string`)

  const headers = Array.isArray(request.header)
    ? request.header.flatMap(header => isRecord(header) && typeof header.key === 'string' ? [header.key.toLowerCase()] : [])
    : []
  const description = optionalString(value, 'description') ?? optionalString(request, 'description')

  return {
    kind: 'request',
    name,
    request: {
      method: (optionalString(request, 'method') ?? 'GET').toUpperCase(),
      rawUrl,
      headers,
      description,
      body: parseBody(request.body, `${context}.request.body`),
      responses: parseResponses(value.response, `${context}.response`),
    },
  }
}

function parseCollection(value: unknown): Collection {
  if (!isRecord(value)) throw new Error('Postman collection must be a JSON object')
  if (!isRecord(value.info)) throw new Error('Postman collection.info must be an object')
  if (!Array.isArray(value.item)) throw new Error('Postman collection.item must be an array')

  const info = value.info
  const variables = new Map<string, string>()
  if (Array.isArray(value.variable)) {
    for (const [index, entry] of value.variable.entries()) {
      if (!isRecord(entry) || typeof entry.key !== 'string' || typeof entry.value !== 'string') {
        throw new Error(`Postman collection.variable[${index}] needs string key and value fields`)
      }
      variables.set(entry.key, entry.value)
    }
  }

  return {
    name: requiredString(info, 'name', 'Postman collection.info'),
    description: optionalString(info, 'description') ?? '',
    postmanId: optionalString(info, '_postman_id'),
    variables,
    items: value.item.map((item, index) => parseItem(item, `Postman collection.item[${index}]`)),
  }
}

function resolveCollectionUrl(rawUrl: string, variables: Map<string, string>): string {
  const baseMatch = /^\{\{([^}]+)\}\}/.exec(rawUrl)
  if (!baseMatch) return rawUrl

  const base = variables.get(baseMatch[1])
  if (!base) throw new Error(`Postman URL uses an unset base variable: ${baseMatch[1]}`)
  return `${base}${rawUrl.slice(baseMatch[0].length)}`
}

function endpointFromRequest(
  item: Extract<CollectionItem, { kind: 'request' }>,
  folder: string,
  variables: Map<string, string>,
): Endpoint | undefined {
  if (excludedPageRequests.has(item.name)) return undefined

  const resolvedUrl = resolveCollectionUrl(item.request.rawUrl, variables)
  const placeholderUrl = resolvedUrl.replace(/\{\{[^}]+\}\}/g, 'placeholder')
  const parsedUrl = new URL(placeholderUrl)
  if (parsedUrl.protocol !== 'http:' && parsedUrl.protocol !== 'https:') return undefined

  const authorityStart = resolvedUrl.indexOf('://') + 3
  const firstPathSlash = resolvedUrl.indexOf('/', authorityStart)
  const pathAndQuery = firstPathSlash < 0 ? '/' : resolvedUrl.slice(firstPathSlash)
  const rawPath = pathAndQuery.split('?', 1)[0] || '/'
  const path = rawPath.replace(/\{\{([^}]+)\}\}/g, '{$1}')
  const queryNames = [...new URLSearchParams(parsedUrl.search).keys()]
  const rawQuery = resolvedUrl.split('?', 2)[1] ?? ''
  const sensitiveQueryName = /(^|_)(access_token|refresh_token|id_token|token|secret|password|client_id|code|code_verifier|username|student_niu|niu|sesid|qr_code|plate_number|scope|redirect_uri)($|_)/i
  const queryExamples = [...new URLSearchParams(rawQuery).entries()]
    .filter(([name, value]) => !value.includes('{{') && !sensitiveQueryName.test(name))
    .map(([name, value]) => ({ name, value }))

  return {
    name: item.name,
    folder,
    method: item.request.method.toLowerCase(),
    path,
    server: parsedUrl.origin,
    queryNames: [...new Set(queryNames)],
    queryExamples,
    request: item.request,
  }
}

function collectEndpoints(collection: Collection): Endpoint[] {
  const endpoints: Endpoint[] = []
  const visit = (items: CollectionItem[], folder = ''): void => {
    for (const item of items) {
      if (item.kind === 'folder') {
        visit(item.children, folder || item.name)
        continue
      }
      const endpoint = endpointFromRequest(item, folder, collection.variables)
      if (endpoint) endpoints.push(endpoint)
    }
  }

  visit(collection.items)
  return endpoints
}

function tagForFolder(folder: string): string {
  if (folder.startsWith('01 - OAuth')) return 'OAuth'
  if (folder.startsWith('02 - Presensi')) return 'Presensi and checkpoint'
  if (folder.startsWith('03 - Legacy')) return 'Legacy microservices'
  if (folder.startsWith('04 - Other first-party')) return 'vNext'
  if (folder.startsWith('05 - Additional')) return 'Low-confidence candidates'
  return 'External integrations'
}

function identifier(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '')
}

function safeFileName(path: string, method: string): string {
  const segments = path.split('/').filter(Boolean).map(segment =>
    segment.replace(/^\{([^}]+)\}$/, '$1').replace(/[^a-zA-Z0-9]+/g, '-').toLowerCase(),
  )
  const base = segments.length === 0 ? 'root' : segments.join('--')
  const suffix = path.endsWith('/') ? '--trailing-slash' : ''
  return `${base}${suffix}-${method}.yaml`
}

function unique(values: string[]): string[] {
  return [...new Set(values)]
}

function yamlScalar(value: SpecValue): string {
  if (value === null) return 'null'
  if (typeof value === 'string') return JSON.stringify(value)
  if (typeof value === 'boolean' || typeof value === 'number') return String(value)
  throw new Error('yamlScalar only accepts scalar values')
}

function yamlStringify(value: SpecValue, indent = 0): string {
  const padding = ' '.repeat(indent)
  if (Array.isArray(value)) {
    if (value.length === 0) return `${padding}[]\n`
    return value.map(entry => {
      if (Array.isArray(entry) || isSpecRecord(entry)) {
        return `${padding}-\n${yamlStringify(entry, indent + 2)}`
      }
      return `${padding}- ${yamlScalar(entry)}\n`
    }).join('')
  }

  if (isSpecRecord(value)) {
    const entries = Object.entries(value)
    if (entries.length === 0) return `${padding}{}\n`
    return entries.map(([key, entry]) => {
      const yamlKey = yamlScalar(key)
      if (Array.isArray(entry) || isSpecRecord(entry)) {
        if (Array.isArray(entry) && entry.length === 0) return `${padding}${yamlKey}: []\n`
        if (isRecord(entry) && Object.keys(entry).length === 0) return `${padding}${yamlKey}: {}\n`
        return `${padding}${yamlKey}:\n${yamlStringify(entry, indent + 2)}`
      }
      return `${padding}${yamlKey}: ${yamlScalar(entry)}\n`
    }).join('')
  }

  return `${padding}${yamlScalar(value)}\n`
}

function parseJsonTemplate(raw: string): unknown | undefined {
  const withSafeValues = raw.replace(/\{\{([^}]+)\}\}/g, (_match, name: string, offset: number, source: string) => {
    let insideString = false
    let escaped = false
    for (const character of source.slice(0, offset)) {
      if (escaped) {
        escaped = false
      } else if (character === '\\') {
        escaped = true
      } else if (character === '"') {
        insideString = !insideString
      }
    }
    if (insideString) return `{{${name}}}`
    const numericNames = new Set(['latitude', 'longitude', 'lat', 'lon', 'accuracy', 'limit', 'skip'])
    return numericNames.has(name.toLowerCase()) ? '0' : JSON.stringify(`{{${name}}}`)
  })
  try {
    const parsed: unknown = JSON.parse(withSafeValues)
    return parsed
  } catch {
    return undefined
  }
}

function schemaFor(value: unknown, propertyName = ''): SpecValue {
  if (value === null) return { type: 'null' }
  if (Array.isArray(value)) {
    return { type: 'array', items: value.length === 0 ? {} : schemaFor(value[0]) }
  }
  if (isRecord(value)) {
    const properties: Record<string, SpecValue> = {}
    for (const [key, entry] of Object.entries(value)) properties[key] = schemaFor(entry, key)
    return { type: 'object', properties, additionalProperties: true }
  }
  if (typeof value === 'boolean') return { type: 'boolean' }
  if (typeof value === 'number') return { type: Number.isInteger(value) ? 'integer' : 'number' }
  if (typeof value === 'string') {
    if (['latitude', 'longitude', 'lat', 'lon', 'accuracy'].includes(propertyName.toLowerCase())) {
      return { type: 'number' }
    }
    return { type: 'string' }
  }
  return {}
}

function redactExample(value: unknown, propertyName = ''): SpecValue {
  const sensitive = /(^|_)(access_token|refresh_token|id_token|token|secret|password|client_id|code|code_verifier|authorization)($|_)/i
  if (sensitive.test(propertyName)) return '<redacted>'
  if (propertyName.toLowerCase() === 'scope') return 'openid user.read userDetail.read'
  if (Array.isArray(value)) return value.map(entry => redactExample(entry))
  if (isRecord(value)) {
    const result: Record<string, SpecValue> = {}
    for (const [key, entry] of Object.entries(value)) result[key] = redactExample(entry, key)
    return result
  }
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    return value
  }
  return '<redacted>'
}

function pathParameters(path: string): SpecValue[] {
  const names = [...path.matchAll(/\{([^}]+)\}/g)].map(match => match[1])
  return names.map(name => ({
    name,
    in: 'path',
    required: true,
    description: 'Path parameter name comes from the Postman placeholder. Its value format is unverified.',
    schema: { type: 'string' },
  }))
}

function queryExampleValue(value: string): SpecValue {
  if (value === 'true') return true
  if (value === 'false') return false
  if (/^-?\d+$/.test(value)) return Number(value)
  if (/^-?\d+\.\d+$/.test(value)) return Number(value)
  return value
}

function queryParameters(group: Endpoint[]): SpecValue[] {
  const first = group[0]
  const names = unique(group.flatMap(endpoint => endpoint.queryNames))
    .filter(name => first.path !== '/oauth/authorize' || name !== 'nonce')
  if (first.path === '/oauth/authorize') {
    for (const name of ['response_type', 'client_id', 'redirect_uri', 'scope', 'state', 'code_challenge', 'code_challenge_method']) {
      if (!names.includes(name)) names.push(name)
    }
  }

  return names.map(name => {
    const safeValues = unique(group.flatMap(endpoint => endpoint.queryExamples
      .filter(example => example.name === name)
      .map(example => example.value)))
    const parameter: Record<string, SpecValue> = {
      name,
      in: 'query',
      description: 'Observed query parameter. Whether it is required is unverified.',
      schema: { type: ['limit', 'skip', 'unit_id', 'banner_app'].includes(name) ? 'integer' : name === 'show_deleted' ? 'boolean' : 'string' },
    }
    if (safeValues.length === 1) {
      const value = safeValues[0]
      if (value !== undefined) parameter.example = queryExampleValue(value)
    } else if (safeValues.length > 1) {
      const examples: Record<string, SpecValue> = {}
      safeValues.forEach((value, index) => {
        examples[`variant${index + 1}`] = { value: queryExampleValue(value) }
      })
      parameter.examples = examples
    }

    if (first.path === '/oauth/authorize') {
      if (['response_type', 'client_id', 'redirect_uri'].includes(name)) {
        parameter.required = true
        parameter.description = 'Required by the UGM ID authorization endpoint schema.'
      }
      if (name === 'response_type') parameter.schema = { type: 'string', enum: ['code'] }
      if (name === 'code_challenge_method') parameter.schema = { type: 'string', enum: ['S256'] }
      if (name === 'redirect_uri') parameter.description = 'Registered loopback redirect URI. The mobile app custom-scheme redirect is not used.'
      if (name === 'scope') parameter.description = 'Request read-only scopes only. The CLI uses `openid user.read` and may add `userDetail.read`.'
      if (name === 'state') parameter.description = 'Optional in the UGM schema; the CLI sends a one-time value and checks it on callback.'
      if (name === 'code_challenge') parameter.description = 'Optional in the UGM schema; required by the CLI PKCE flow.'
      if (name === 'code_challenge_method') parameter.description = 'Optional in the UGM schema; the CLI PKCE flow uses S256.'
    } else if (['token', 'password', 'code', 'code_verifier'].includes(name)) {
      parameter.description = 'Sensitive query parameter. Do not log or commit its value.'
    }
    return parameter
  })
}

function bodyContent(endpointGroup: Endpoint[]): SpecValue | undefined {
  const withBody = endpointGroup.filter(endpoint => endpoint.request.body.kind !== 'none')
  if (withBody.length === 0) return undefined
  const first = withBody[0]
  const body = first.request.body

  if (body.kind === 'urlencoded') {
    const properties: Record<string, SpecValue> = {}
    for (const field of body.fields) {
      if (field === 'grant_type') {
        properties[field] = { type: 'string', enum: ['authorization_code'] }
      } else if (first.path === '/oauth/token') {
        const descriptions: Record<string, string> = {
          code: 'Authorization code returned by the authorization endpoint; used with grant_type=authorization_code.',
          redirect_uri: 'Registered loopback redirect URI used for the authorization request.',
          client_id: 'Client identifier configured for this CLI installation.',
          code_verifier: 'PKCE verifier matching the authorization request code_challenge; required by the CLI PKCE flow.',
        }
        properties[field] = {
          type: 'string',
          ...(descriptions[field] === undefined ? {} : { description: descriptions[field] }),
        }
      } else {
        properties[field] = { type: 'string' }
      }
    }
    return {
      required: true,
      ...(first.path === '/oauth/token'
        ? { description: 'UGM documents grant_type as the only universally required form field. This operation describes the CLI Authorization Code + PKCE exchange, which also sends code, redirect_uri, client_id, and code_verifier.' }
        : {}),
      content: {
        'application/x-www-form-urlencoded': {
          schema: {
            type: 'object',
            properties,
            required: first.path === '/oauth/token' ? ['grant_type'] : body.fields,
            additionalProperties: false,
          },
        },
      },
    }
  }

  if (body.kind === 'raw') {
    const firstValue = parseJsonTemplate(body.value)
    const mediaType: Record<string, SpecValue> = {
      schema: firstValue === undefined ? {} : schemaFor(firstValue),
    }
    const examples: Record<string, SpecValue> = {}
    for (const endpoint of withBody) {
      if (endpoint.request.body.kind !== 'raw') continue
      const value = parseJsonTemplate(endpoint.request.body.value)
      if (value === undefined) continue
      const key = identifier(endpoint.name) || 'request'
      examples[key] = {
        summary: endpoint.name,
        value: redactExample(value),
      }
    }
    if (Object.keys(examples).length > 0) mediaType.examples = examples
    return {
      required: true,
      description: 'Request body fields come from the supplied Postman requests. Field requirements remain unverified unless documented by UGM.',
      content: { 'application/json': mediaType },
    }
  }

  return undefined
}

function responseObject(response: PostmanResponse): SpecValue {
  const description = `${response.name}. The collection response is illustrative and was not live-tested.`
  if (!response.body) return { description }

  try {
    const parsed: unknown = JSON.parse(response.body)
    const safe = redactExample(parsed)
    return {
      description,
      content: {
        'application/json': {
          schema: schemaFor(safe),
          examples: { illustrative: { summary: response.name, value: safe } },
        },
      },
    }
  } catch {
    return {
      description,
      content: { 'text/plain': { schema: { type: 'string' }, example: '<redacted>' } },
    }
  }
}

function statusFor(path: string): 'documented by UGM' | 'unverified' {
  return path === '/oauth/authorize' || path === '/oauth/token' ? 'documented by UGM' : 'unverified'
}

function operationFor(group: Endpoint[], collection: Collection): SpecValue {
  const first = group[0]
  const tag = tagForFolder(first.folder)
  const descriptions = unique(group.map(endpoint => endpoint.request.description?.trim() ?? '').filter(Boolean))
  const status = statusFor(first.path)
  let description = descriptions.length > 0
    ? descriptions.join('\n\n')
    : 'The supplied Postman collection records this request from static APK analysis. It contains no live response for this route.'
  if (status === 'documented by UGM') {
    description += '\n\nUGM documents this endpoint in its [OAuth 2.0/OIDC specification](https://oauth.simaster.ugm.ac.id/openapi.json). The Postman request is static evidence and was not live-tested.'
  } else if (tag === 'Low-confidence candidates') {
    description += '\n\nThis is a low-confidence route candidate. The APK string does not establish the host, method, authentication, or response contract unless the request description says otherwise.'
  }
  if (first.path === '/oauth/authorize') {
    description += '\n\nThe UGM schema requires `response_type`, `client_id`, and `redirect_uri`; `scope`, `state`, `code_challenge`, and `code_challenge_method` are optional in that schema. The CLI sends `state`, PKCE parameters, and an OIDC `nonce` for its own validation; UGM does not list `nonce` as an authorization query parameter.'
  } else if (first.path === '/oauth/token') {
    description += '\n\nThe UGM schema lists only `grant_type` as universally required. This catalog describes the CLI Authorization Code + PKCE exchange; the CLI supplies the authorization code, registered redirect URI, client identifier, and PKCE verifier for that grant.'
  }

  const servers = unique(group.map(endpoint => endpoint.server)).map(url => ({
    url,
    description: url.includes('.dev.') ? 'Development host observed in the APK collection; unverified.' : 'Host recorded in the supplied collection.',
  }))
  const responses: Record<string, SpecValue> = {}
  for (const endpoint of group) {
    for (const response of endpoint.request.responses) {
      if (response.code !== undefined) responses[String(response.code)] = responseObject(response)
    }
  }
  if (first.path === '/oauth/authorize') {
    responses['302'] = {
      description: 'Redirects through UGM CAS SSO and consent, then returns to the registered redirect URI.',
      headers: { Location: { schema: { type: 'string' } } },
    }
  }
  if (Object.keys(responses).length === 0) {
    responses.default = { description: 'Response status and body are not established by the supplied collection.' }
  }

  const operation: Record<string, SpecValue> = {
    tags: [tag],
    summary: first.name,
    operationId: identifier(`${tag}_${first.method}_${first.path}`),
    description,
    servers,
  }
  const parameters = [...pathParameters(first.path), ...queryParameters(group)]
  if (parameters.length > 0) operation.parameters = parameters
  if (group.some(endpoint => endpoint.request.headers.includes('authorization'))) {
    operation.security = [{ bearerAuth: [] }]
  }
  const requestBody = bodyContent(group)
  if (requestBody !== undefined) operation.requestBody = requestBody
  operation.responses = responses
  operation['x-status'] = status
  operation['x-live-tested'] = false

  const provenance: Record<string, SpecValue> = {
    source: `${collection.name}, supplied Postman collection generated from supplied APK splits on ${collectionDate(collection.description)}`,
    requests: group.map(endpoint => endpoint.name),
  }
  if (status === 'documented by UGM') {
    provenance.officialDocumentation = 'https://oauth.simaster.ugm.ac.id/openapi.json'
  }
  operation['x-provenance'] = provenance
  return operation
}

function pathItemGroups(endpoints: Endpoint[]): Map<string, Endpoint[]> {
  const groups = new Map<string, Endpoint[]>()
  for (const endpoint of endpoints) {
    const key = `${endpoint.path}\n${endpoint.method}`
    groups.set(key, [...(groups.get(key) ?? []), endpoint])
  }
  return groups
}

function collectionDate(description: string): string {
  return /\b(20\d{2}-\d{2}-\d{2})\b/.exec(description)?.[1] ?? 'unknown date'
}

function outputFiles(collection: Collection): Map<string, string> {
  const endpoints = collectEndpoints(collection)
  const groups = [...pathItemGroups(endpoints).values()].sort((left, right) =>
    `${left[0].path}\n${left[0].method}`.localeCompare(`${right[0].path}\n${right[0].method}`),
  )
  const fileNames = new Map<string, string>()
  const pathObjects: Record<string, SpecValue> = {}
  const generatedPaths = new Map<string, string>()

  for (const group of groups) {
    const first = group[0]
    const fileName = safeFileName(first.path, first.method)
    const priorPath = fileNames.get(fileName)
    if (priorPath && priorPath !== first.path) {
      throw new Error(`Path filename collision between ${priorPath} and ${first.path}`)
    }
    fileNames.set(fileName, first.path)
    generatedPaths.set(join(pathsRoot, fileName), yamlStringify({ [first.method]: operationFor(group, collection) }))
    pathObjects[first.path] = { $ref: `./paths/${fileName}` }
  }

  const apiBase = collection.variables.get('api_base')
  if (!apiBase) throw new Error('Postman collection must define the api_base variable')

  const infoDescription = [
    `Catalog generated from the supplied Postman collection, which records static APK analysis dated ${collectionDate(collection.description)} and no live traffic.`,
    'The collection mixes UGM ID OAuth endpoints, SIMASTER vNext routes, legacy microservices, low-confidence route candidates, and external integrations.',
    'Duplicate requests with the same method and path share one operation. Saved response bodies are illustrative.',
    'Only the OAuth authorization and token endpoints are marked documented by UGM. All operations have x-live-tested: false.',
    'This reference does not authorize live requests. Use only an authorized environment and your own account.',
    `Page-navigation requests named ${[...excludedPageRequests].join(', ')} are omitted.`,
    'The Armada GPS WebSocket URL is omitted because OpenAPI does not describe its message frames or connection lifecycle.',
  ].join(' ')

  const openApi: SpecValue = {
    openapi: '3.1.0',
    info: {
      title: 'SIMASTER Student vNext and related API catalog',
      version: '0.1.0',
      summary: 'Static APK-derived endpoint catalog. Endpoint behavior is unverified unless marked otherwise.',
      description: infoDescription,
    },
    servers: [{ url: apiBase, description: 'Host recorded in the supplied Postman collection.' }],
    tags: [
      { name: 'OAuth', description: 'UGM ID OAuth 2.0 and OIDC endpoints. The route contract is documented by UGM.' },
      { name: 'Presensi and checkpoint', description: 'Attendance and checkpoint routes found in APK static analysis. These include write-like operations; contracts are unverified.' },
      { name: 'Legacy microservices', description: 'Student and staff microservice routes and service names found in APK static analysis.' },
      { name: 'vNext', description: 'First-party vNext paths recorded in the supplied collection.' },
      { name: 'Low-confidence candidates', description: 'Route strings from static analysis with incomplete host, method, authentication, or response contract evidence.' },
      { name: 'External integrations', description: 'External services and SDK endpoints referenced by the mobile application.' },
    ],
    paths: pathObjects,
    components: {
      securitySchemes: {
        bearerAuth: {
          type: 'http',
          scheme: 'bearer',
          bearerFormat: 'OAuth 2.0 access token',
          description: 'Authorization: Bearer access token. Collection evidence does not establish this requirement for endpoints without the header.',
        },
      },
    },
    'x-source-collection': {
      name: collection.name,
      description: collection.description,
      ...(collection.postmanId === undefined ? {} : { 'postman-id': collection.postmanId }),
    },
  }

  generatedPaths.set(join(outputRoot, 'openapi.yaml'), yamlStringify(openApi))
  generatedPaths.set(humanReferencePath, humanReference(collection, endpoints.length, groups.length))
  return generatedPaths
}

function humanReference(collection: Collection, requestCount: number, pathCount: number): string {
  return `# SIMASTER vNext API reference

[\`openapi/simaster-vnext/openapi.yaml\`](openapi/simaster-vnext/openapi.yaml) describes the API requests in the supplied Postman collection.

The collection \`${collection.name}\` records static analysis of supplied SIMASTER Student APK splits on ${collectionDate(collection.description)}. It contains no live traffic. The OpenAPI file combines ${requestCount} HTTP requests into ${pathCount} path items and keeps the original Postman request names as provenance.

## Evidence

Each operation has \`x-status\`, \`x-live-tested\`, and \`x-provenance\` fields.

- \`documented by UGM\` marks the UGM ID OAuth authorization and token endpoints. See the [official UGM ID OpenAPI specification](https://oauth.simaster.ugm.ac.id/openapi.json).
- \`unverified\` marks routes derived from APK static analysis or external integration strings.
- \`x-live-tested\` is \`false\` for every operation. The collection contains no live-tested response.
- Saved response bodies are illustrative examples from the collection. They do not establish the live response contract.

The catalog includes vNext routes, checkpoint and attendance routes, legacy microservices, low-confidence route candidates, and external integrations. Low-confidence candidates may have an unknown host, method, authentication requirement, or response shape. A request header missing from the collection does not establish that an endpoint is public.

The catalog omits six website page requests: SIMASTER services, the SIMASTER home page, Report, ULT, RSA clinic, and the DTI knowledge base. It also omits the Armada GPS WebSocket URL because OpenAPI does not describe WebSocket messages or connection lifecycle.

## Use limits

This reference does not authorize live requests. Do not use it to access another person's data or to bypass authentication, CAPTCHA, MFA, rate limits, or authorization. Use only an authorized environment and your own account.

The OAuth operations describe the CLI's Authorization Code flow with PKCE and a registered loopback redirect. They do not include the mobile app's client ID, embedded secret, custom-scheme redirect, or write scopes. The CLI requests read-only scopes only.

The checkpoint and legacy attendance catalog includes POST operations that may change attendance records. Their presence documents APK-derived route claims only. The SIMASTER CLI does not implement these writes.

## Regenerate

Run \`bun scripts/sync-spec.ts path/to/SIMASTER.postman_collection.json\` to regenerate the OpenAPI files and this reference. Add \`--check\` before the collection path to report drift without writing files.

Run \`bun test scripts/sync-spec.test.ts\` to check generation, redaction, OAuth contracts, and drift handling in an isolated temporary directory.
`
}

async function writeOutputs(outputs: Map<string, string>): Promise<void> {
  await mkdir(pathsRoot, { recursive: true })
  const desiredPathFiles = new Set([...outputs.keys()].filter(path => dirname(path) === pathsRoot))
  for (const name of await readdir(pathsRoot)) {
    const filePath = join(pathsRoot, name)
    if (name.endsWith('.yaml') && !desiredPathFiles.has(filePath)) await rm(filePath, { force: true })
  }
  for (const [filePath, content] of outputs) {
    await mkdir(dirname(filePath), { recursive: true })
    await writeFile(filePath, content)
  }
}

async function checkOutputs(outputs: Map<string, string>): Promise<void> {
  const stale: string[] = []
  for (const [filePath, expected] of outputs) {
    let actual: string
    try {
      actual = await readFile(filePath, 'utf8')
    } catch {
      stale.push(filePath)
      continue
    }
    if (actual !== expected) stale.push(filePath)
  }

  for (const name of await readdir(pathsRoot).catch(() => [])) {
    const filePath = join(pathsRoot, name)
    if (name.endsWith('.yaml') && !outputs.has(filePath)) stale.push(filePath)
  }

  if (stale.length > 0) {
    process.stderr.write(`Generated files are out of date:\n${stale.map(path => `  ${path}`).join('\n')}\n`)
    process.exitCode = 1
    return
  }
  process.stdout.write(`OpenAPI files match the collection (${outputs.size} generated files).\n`)
}

async function main(): Promise<void> {
  const args = process.argv.slice(2)
  const checkOnly = args[0] === '--check'
  const input = checkOnly ? args[1] : args[0]
  if (!input || args.length !== (checkOnly ? 2 : 1)) {
    throw new Error('Usage: bun scripts/sync-spec.ts [--check] <postman-collection.json>')
  }

  const collectionFile = resolve(input)
  const sourceText = await readFile(collectionFile, 'utf8')
  const parsedSource: unknown = JSON.parse(sourceText)
  const collection = parseCollection(parsedSource)
  const outputs = outputFiles(collection)
  if (checkOnly) {
    await checkOutputs(outputs)
    return
  }

  await writeOutputs(outputs)
  process.stdout.write(`Generated ${outputs.size} files from ${basename(collectionFile)}.\n`)
}

if (import.meta.main) {
  main().catch(error => {
    const message = error instanceof Error ? error.message : 'Unexpected error'
    process.stderr.write(`${message}\n`)
    process.exitCode = 1
  })
}
