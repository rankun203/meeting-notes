---
name: download-teams-recording
description: Adaptively download authorized Microsoft Teams meeting recordings hosted by SharePoint, OneDrive, Stream, or Clipchamp into the user's Downloads folder. Use when a user provides a recording link, cannot find a download button, has view-only playback access, or supplies a HAR capture. Inspect the current site and network traffic instead of relying on fixed private endpoints or a long-lived downloader script.
---

# Download Teams Recording

Save an authorized Teams meeting recording to `~/Downloads`. Assume FFmpeg is installed. Treat the user as unfamiliar with browser developer tools and explain one step at a time.

## Workflow

1. Confirm the user is signed in to the Microsoft account that can play the recording and is authorized to retain it.
2. Open the supplied recording link using the available browser-control skill.
3. Look for a visible **Download** action in the recording title menu or **More options** menu.
4. If Microsoft permits the normal download, use it, save the file in `~/Downloads`, verify it with `ffprobe`, and stop.
5. If the page is view-only or omits Download, inspect the current page and network behavior before choosing a fallback.

Do not ask the user to find or copy media URLs, cookies, encryption keys, or request headers.

## Observe the current player

Private Microsoft media APIs change. Do not assume that endpoint names, manifest schemas, encryption, segment numbering, or authentication from a previous recording still apply.

Use the best evidence currently available:

- Inspect requests and response bodies from the signed-in browser session when browser tooling exposes full network traffic.
- If the user already supplied a HAR, inspect it locally without printing secrets.
- Otherwise guide the user through exporting a HAR with response content.

Before writing download code, identify the present recording's manifest format, audio and video representations, segment addressing, authentication requirements, encryption metadata, duration, and whether segments appear only as playback advances.

Treat each media URL emitted by the player as an opaque, playback-scoped capability. Do not assume that changing a segment number, timestamp, track, or other query parameter creates an authorized request for another fragment. Even when a modified URL happens to work for nearby fragments, do not build the download around that behavior.

## Prove the encryption bootstrap first

Do not begin the full 2x playback pass until the recording's startup traffic has been captured and validated. Start observing Network traffic before reloading the page so the capture includes the requests and responses that initialize playback, not only later media fragments.

Before continuing through the recording, confirm that the available browser capture or HAR is complete and contains enough current evidence to identify:

- the media manifest and selected audio/video tracks;
- the current recording's encryption metadata and the request/response or player state that supplies its decryption material;
- the initialization segment for each selected track; and
- at least one encrypted media fragment from each selected track.

Fetch and decrypt a sample audio fragment and video fragment, combine each with its initialization data as required, and confirm FFmpeg can parse or decode both. This is a hard readiness gate: if the decryption material, initialization data, or sample validation is missing, capture a fresh page reload immediately. Do not play the whole recording first and discover afterward that only unusable encrypted fragments were saved.

Loading the recording URL is sufficient only when the available browser tooling exposes the complete network requests and response bodies needed for this validation. Visible-page control, DOM inspection, or a list of asset URLs alone is not sufficient. If full response capture is unavailable, ask the user for a HAR with content at the start of the workflow.

## Ask for a HAR when needed

Guide the user through these steps one at a time:

1. Open the recording in Chrome or Edge and confirm it plays.
2. Open Developer Tools:
   - macOS: press `Option+Command+I`.
   - Windows: press `F12` or `Ctrl+Shift+I`.
3. Select the **Network** tab and enable **Preserve log**.
4. Reload the recording page and press Play.
5. Open the player's speed control and select **2x**. Leave the tab playing.
6. After playback has started and the first audio/video fragments appear, right-click inside the Network request list and choose **Save all as HAR with content**. This initial HAR must capture the page reload and playback bootstrap, including the encryption-key or equivalent decryption-material response.
7. Ask for the saved HAR's local path. Keep the recording tab open and playing while the download runs.

If browser control is available, handle opening the page, starting playback, and selecting 2× directly. Capture the startup requests and responses directly only when the browser tooling exposes their full bodies; otherwise have the user export the initial HAR. After the encryption bootstrap passes validation, capture the exact media request URLs as the player emits them and download each fragment promptly before its playback-scoped URL expires.

Treat the HAR as sensitive authentication material. Never print its full contents, signed URLs, cookies, or `x-spopactoken` values. Ask before deleting the user's HAR after completion.

## Build only what this recording needs

After the encryption-bootstrap validation passes, write the smallest one-off downloader that matches the current evidence. Put temporary code and segments in a dedicated directory under `/private/tmp`; do not add the downloader to the skill or repository.

Prefer the highest-quality original-copy audio and video representations. If the recording is segmented, make the temporary program resumable and tolerant of SharePoint's transcode-ahead boundary. Consume only the exact segment URLs observed from the signed-in player; never synthesize additional media URLs by rewriting their parameters. Download, decrypt when required, and validate every observed fragment immediately, retaining completed fragments across retries. When segments are encrypted, derive the required decryption procedure only from the current manifest and captured requests. Never embed or log credentials.

Keep the player advancing continuously at 2× when SharePoint only emits media URLs near the playback position. Observe requests at a restrained rate without seeking around the timeline, and continue until the player reaches the end. Deduplicate captured URLs by the manifest's segment identity, verify complete audio and video timeline coverage, and only then construct the tracks and remux the final recording. Report concise progress during long downloads.

Use FFmpeg to remux compatible tracks without re-encoding. Choose a collision-safe `.mp4` filename based on the recording title and write it to `~/Downloads`. Request filesystem approval when required.

Verify the result with both `ffprobe` and a full FFmpeg decode pass. Remove the temporary program and segment directory after success, but do not delete the user's HAR without permission.

## Troubleshooting

- **No usable manifest found**: repeat the HAR capture, ensuring **with content** was selected after reloading and playing the recording.
- **No decryption material was captured**: stop before the long playback pass. Open Network with Preserve log enabled, reload the page, start playback, and export a fresh HAR with content after the first media fragments appear.
- **Authentication, key, or initialization fetch fails**: the signed capture may have expired. Export a fresh bootstrap HAR and retry promptly; revalidate sample audio and video before continuing.
- **Waiting at a segment**: resume the recording tab, keep it in the signed-in browser, confirm 2× playback is still active, and download the exact URL when the player emits it. Do not rewrite a previously captured URL to request the missing segment.
- **The player ended but requests still fail**: reload, play again at 2×, export a fresh HAR, and reuse already validated temporary segments when safe.
- **Normal download returns 401/403**: do not guess endpoints or expose credentials; return to current browser/network evidence.

## Completion

Report the clickable output path, verified duration, codecs, resolution, and file size. If reconstructed from streamed tracks, explain that the result is a remux and may not be byte-identical to Microsoft's source container.
