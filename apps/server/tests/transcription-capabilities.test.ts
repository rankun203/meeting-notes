import assert from 'node:assert/strict'
import { test } from 'node:test'
import { transcriptionLanguages } from '../src/server/transcription-capabilities'

const config = {
  endpoint: 'https://worker.example.test/v2/audio',
  key: 'test-secret',
}
const catalog = {
  protocolVersion: 1,
  transcription: {
    languages: [
      { code: 'en', name: 'English' },
      { code: 'zh-cn', name: 'Chinese (Simplified)' },
    ],
  },
}

test('RunPod discovers languages using only the audio-free metadata operation', async () => {
  const request = (async (url, options) => {
    assert.equal(url, `${config.endpoint}/runsync`)
    assert.equal(options?.method, 'POST')
    assert.deepEqual(JSON.parse(String(options?.body)), {
      input: { operation: 'capabilities' },
    })
    assert.equal(
      new Headers(options?.headers).get('Authorization'),
      'Bearer test-secret',
    )
    assert.equal(options?.redirect, 'error')
    return Response.json({ status: 'COMPLETED', output: catalog })
  }) as typeof fetch
  assert.deepEqual(await transcriptionLanguages(config, request), {
    transcriptionLanguages: catalog.transcription.languages,
    transcriptionLanguagesError: null,
  })
})

test('local workers expose the same catalog through authenticated GET', async () => {
  const request = (async (url, options) => {
    assert.equal(url, 'https://worker.example.test/capabilities')
    assert.equal(options?.method, 'GET')
    assert.equal(options?.body, undefined)
    return Response.json(catalog)
  }) as typeof fetch
  const result = await transcriptionLanguages(
    { ...config, endpoint: 'https://worker.example.test', provider: 'local' },
    request,
  )
  assert.deepEqual(
    result.transcriptionLanguages,
    catalog.transcription.languages,
  )
})

test('missing, old, pending, rejected and malformed workers never receive a fallback catalog', async () => {
  const noRequest = (async () => {
    throw new Error('must not request')
  }) as typeof fetch
  assert.equal(
    (await transcriptionLanguages(null, noRequest)).transcriptionLanguages,
    null,
  )
  let index = 0
  for (const response of [
    Response.json({ status: 'IN_QUEUE', id: '../invalid' }),
    Response.json({ status: 'COMPLETED', output: { tracks: {} } }),
    Response.json({
      status: 'COMPLETED',
      output: { ...catalog, protocolVersion: 2 },
    }),
    Response.json({
      status: 'COMPLETED',
      output: {
        protocolVersion: 1,
        transcription: { languages: [{ code: 'auto', name: 'Automatic' }] },
      },
    }),
    Response.json({
      status: 'COMPLETED',
      output: {
        protocolVersion: 1,
        transcription: {
          languages: [
            catalog.transcription.languages[0],
            catalog.transcription.languages[0],
          ],
        },
      },
    }),
    Response.json({ error: 'private-url-and-key' }, { status: 401 }),
  ]) {
    const result = await transcriptionLanguages(
      { ...config, endpoint: `https://worker.example.test/invalid-${index++}` },
      (async () => response) as typeof fetch,
    )
    assert.equal(result.transcriptionLanguages, null)
    assert.ok(result.transcriptionLanguagesError)
    assert.ok(!JSON.stringify(result).includes('private-url-and-key'))
  }
})

test('cold RunPod metadata polls the accepted job and caches across concurrent callers', async () => {
  const calls: string[] = []
  const coldConfig = { ...config, endpoint: 'https://worker.example.test/cold' }
  const request = (async (url) => {
    calls.push(String(url))
    return Response.json(
      String(url).endsWith('/runsync')
        ? { status: 'IN_QUEUE', id: 'metadata-job' }
        : { status: 'COMPLETED', output: catalog },
    )
  }) as typeof fetch
  const results = await Promise.all([
    transcriptionLanguages(coldConfig, request),
    transcriptionLanguages(coldConfig, request),
  ])
  assert.ok(
    results.every((value) => value.transcriptionLanguages?.length === 2),
  )
  await transcriptionLanguages(coldConfig, request)
  assert.deepEqual(calls, [
    `${coldConfig.endpoint}/runsync`,
    `${coldConfig.endpoint}/status/metadata-job`,
  ])
  await transcriptionLanguages({ ...coldConfig, key: 'different-key' }, request)
  assert.equal(calls.length, 4)
})

test('metadata failures are briefly cached without exposing worker errors', async () => {
  let calls = 0
  const request = (async () => {
    calls++
    throw new Error('private worker detail')
  }) as typeof fetch
  const failed = { ...config, endpoint: 'https://worker.example.test/failing' }
  const first = await transcriptionLanguages(failed, request)
  const second = await transcriptionLanguages(failed, request)
  assert.deepEqual(second, first)
  assert.equal(calls, 1)
  assert.equal(first.transcriptionLanguages, null)
  assert.ok(!JSON.stringify(first).includes('private worker detail'))
  await transcriptionLanguages({ ...failed, provider: 'local' }, request)
  assert.equal(calls, 2)
})
