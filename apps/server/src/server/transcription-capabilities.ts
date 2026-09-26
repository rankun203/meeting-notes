import { createHash } from 'node:crypto'
import { z } from 'zod'

const language = z.object({
  code: z
    .string()
    .min(1)
    .max(40)
    .regex(/^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/),
  name: z.string().trim().min(1).max(120),
})
const metadata = z.object({
  protocolVersion: z.literal(1),
  transcription: z.object({ languages: z.array(language).min(1).max(256) }),
})
type WorkerConfiguration = {
  endpoint: string
  key: string
  provider?: 'runpod' | 'local'
}
type Result = {
  transcriptionLanguages: z.infer<typeof language>[] | null
  transcriptionLanguagesError: string | null
}
const cache = new Map<
  string,
  { expires: number; result?: Result; pending?: Promise<Result> }
>()
const unavailable = (): Result => ({
  transcriptionLanguages: null,
  transcriptionLanguagesError:
    'The transcription worker did not provide supported languages. Check its connection and protocol version.',
})

async function discover(
  config: WorkerConfiguration,
  request: typeof fetch,
): Promise<Result> {
  try {
    const local = config.provider === 'local'
    const endpoint = config.endpoint.replace(/\/$/, '')
    const headers = {
      Authorization: `Bearer ${config.key}`,
      'Content-Type': 'application/json',
    }
    const signal = AbortSignal.timeout(60_000)
    const call = async (path: string, body?: object) => {
      const response = await request(`${endpoint}/${path}`, {
        method: body ? 'POST' : 'GET',
        headers,
        ...(body ? { body: JSON.stringify(body) } : {}),
        signal,
        cache: 'no-store',
        redirect: 'error',
      })
      if (!response.ok) throw new Error('Metadata request failed')
      return response.json()
    }
    let body = await call(
      local ? 'capabilities' : 'runsync',
      local ? undefined : { input: { operation: 'capabilities' } },
    )
    // A cold RunPod worker may return a job before metadata is ready. Poll that
    // same job; do not submit another one while waiting.
    for (
      let poll = 0;
      !local && ['IN_QUEUE', 'IN_PROGRESS'].includes(body?.status) && poll < 30;
      poll++
    ) {
      if (
        typeof body.id !== 'string' ||
        !/^[A-Za-z0-9_-]{1,200}$/.test(body.id)
      )
        throw new Error('Invalid metadata job ID')
      const jobID = body.id
      await new Promise((resolve) => setTimeout(resolve, 500))
      body = await call(`status/${encodeURIComponent(jobID)}`)
      // Some status responses omit the already-known identifier.
      if (!body.id) body.id = jobID
    }
    if (!local && body?.status !== 'COMPLETED')
      throw new Error('Metadata is not ready')
    const result = metadata.parse(local ? body : body.output)
    const languages = result.transcription.languages
    if (
      languages.some((item) => item.code === 'auto') ||
      new Set(languages.map((item) => item.code)).size !== languages.length
    )
      throw new Error('Invalid language catalog')
    return {
      transcriptionLanguages: languages,
      transcriptionLanguagesError: null,
    }
  } catch {
    return unavailable() // Never expose credentials or untrusted worker messages.
  }
}

/** Audio-free discovery, cached by exact worker configuration with in-flight deduplication. */
export async function transcriptionLanguages(
  config: WorkerConfiguration | null,
  request: typeof fetch = fetch,
): Promise<Result> {
  if (!config)
    return {
      transcriptionLanguages: null,
      transcriptionLanguagesError: 'Transcription is not configured.',
    }
  const key = createHash('sha256')
    .update(
      JSON.stringify([
        config.endpoint,
        config.key,
        config.provider ?? 'runpod',
      ]),
    )
    .digest('hex')
  const existing = cache.get(key)
  if (existing?.pending) return existing.pending
  if (existing?.result && existing.expires > Date.now())
    return structuredClone(existing.result)
  // Bound memory even if deployment configuration changes repeatedly.
  if (cache.size >= 64) {
    for (const [id, entry] of cache)
      if (!entry.pending && entry.expires <= Date.now()) cache.delete(id)
    if (cache.size >= 64) {
      const oldest = [...cache].find(([, entry]) => !entry.pending)
      if (oldest) cache.delete(oldest[0])
    }
  }
  const pending = discover(config, request)
  cache.set(key, { expires: 0, pending })
  const result = await pending
  cache.set(key, {
    expires: Date.now() + (result.transcriptionLanguages ? 300_000 : 5_000),
    result,
  })
  return structuredClone(result)
}
