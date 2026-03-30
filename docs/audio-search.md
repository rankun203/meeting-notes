# Audio Search: Approaches & Findings

## Problem

Search meeting audio using text queries with high accuracy, even when transcription errors occur (e.g., "Wiredcraft" transcribed as "will craft").

## Approaches Evaluated

### 1. CLAP (Contrastive Language-Audio Pretraining)

Audio equivalent of CLIP — embeds audio and text into a shared vector space.

- **Models**: `laion/larger_clap_general`, `laion/larger_clap_music_and_speech`
- **Size**: ~0.3-0.5B params, ~2-4GB VRAM
- **Library**: HuggingFace `transformers` (`ClapModel`, `ClapProcessor`)
- **Verdict**: Not suitable for speech search. Trained on general audio-text pairs ("dog barking", "jazz music"), not precise speech content. Would match vibes, not words.

### 2. Text Embedding Search on Transcripts

Embed WhisperX transcript segments with a text embedding model, search via vector similarity.

- **Pros**: Semantic search works ("budget discussion" finds "we need to cut costs by 20%"). Fast. We already have word-level timestamps from WhisperX.
- **Cons**: Fails on transcription errors. "Wiredcraft" → "will craft" has no semantic overlap. Text embeddings (Qwen3, sentence-transformers, etc.) operate on text as written — they cannot recover from upstream transcription mistakes.

### 3. Whisper Encoder Embeddings

Use Whisper's encoder hidden states (before decoding to text) as audio embeddings. These capture what was actually said, not what was transcribed.

- **Pros**: No new model needed (already running Whisper). Embeddings are phonetically faithful.
- **Cons**: Query-side problem — how to embed search text into the same audio space? Would need TTS or text-to-phoneme conversion. Open research problem with no production-ready solution.

### 4. Phonetic Search

Convert transcript and query to phonetic representations (Soundex, Metaphone, phoneme sequences). "Wiredcraft" and "will craft" sound similar → phonetic match.

- **Pros**: Handles transcription errors for similar-sounding words.
- **Cons**: Loses semantic meaning. Only works for phonetically similar errors, not omissions or rewordings.

### 5. Fix Transcription at Source (Recommended First Step)

Improve Whisper's accuracy for known vocabulary so the transcript is correct from the start, making text search reliable.

- **`initial_prompt`**: Bias Whisper toward expected terms by providing context/vocabulary in the prompt
- **Custom vocabulary / hotwords**: faster-whisper supports `hotwords` parameter to boost recognition of specific terms (company names, jargon, people's names)
- **Pros**: Fixes the root cause. Text search just works when transcription is correct.
- **Cons**: Requires maintaining a vocabulary list per user/org.

## Recommended Strategy

**Short term**: Fix transcription accuracy using Whisper's vocabulary biasing (`hotwords`, `initial_prompt`). This makes text-based search on transcripts reliable for known terms.

**Medium term**: Hybrid search combining:
1. Text embedding search on transcripts (semantic matching)
2. Phonetic fallback for low-confidence words (WhisperX already returns `score` per word — low scores indicate likely mistranscriptions)

**Long term**: Audio embedding search if/when production-ready speech-text cross-modal models mature.

## Existing Pipeline State

The current audio extraction pipeline (`apps/audio-extraction/`) already returns:
- Word-level timestamps with confidence scores from WhisperX
- Speaker diarization with speaker identity embeddings (256-dim, pyannote) — useful for speaker matching, not content search
- Per-word `score` field that can flag likely transcription errors
