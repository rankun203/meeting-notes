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

- Inspect requests from the signed-in browser session when browser tooling exposes them.
- If the user already supplied a HAR, inspect it locally without printing secrets.
- Otherwise guide the user through exporting a HAR with response content.

Before writing download code, identify the present recording's manifest format, audio and video representations, segment addressing, authentication requirements, encryption metadata, duration, and whether segments appear only as playback advances.

## Ask for a HAR when needed

Guide the user through these steps one at a time:

1. Open the recording in Chrome or Edge and confirm it plays.
2. Open Developer Tools:
   - macOS: press `Option+Command+I`.
   - Windows: press `F12` or `Ctrl+Shift+I`.
3. Select the **Network** tab and enable **Preserve log**.
4. Reload the recording page and press Play.
5. Open the player's speed control and select **2x**. Leave the tab playing.
6. After several seconds, right-click inside the Network request list and choose **Save all as HAR with content**.
7. Ask for the saved HAR's local path. Keep the recording tab open and playing while the download runs.

If browser control is available, handle opening the page, starting playback, and selecting 2× directly. The user still needs to export the HAR if browser Developer Tools are not controllable.

Treat the HAR as sensitive authentication material. Never print its full contents, signed URLs, cookies, or `x-spopactoken` values. Ask before deleting the user's HAR after completion.

## Build only what this recording needs

After inspecting the current evidence, write the smallest one-off downloader that matches it. Put temporary code and segments in a dedicated directory under `/private/tmp`; do not add the downloader to the skill or repository.

Prefer the highest-quality original-copy audio and video representations. If the recording is segmented, make the temporary program resumable and tolerant of SharePoint's transcode-ahead boundary. When segments are encrypted, derive the required decryption procedure only from the current manifest and captured requests. Never embed or log credentials.

Keep the player advancing at 2× when SharePoint only makes media available near the playback position. Poll at a restrained rate, retain completed segments across retries, and report concise progress during long downloads.

Use FFmpeg to remux compatible tracks without re-encoding. Choose a collision-safe `.mp4` filename based on the recording title and write it to `~/Downloads`. Request filesystem approval when required.

Verify the result with both `ffprobe` and a full FFmpeg decode pass. Remove the temporary program and segment directory after success, but do not delete the user's HAR without permission.

## Troubleshooting

- **No usable manifest found**: repeat the HAR capture, ensuring **with content** was selected after reloading and playing the recording.
- **Authentication, key, or initialization fetch fails**: the signed capture may have expired. Export a fresh HAR and retry promptly.
- **Waiting at a segment**: resume the recording tab, keep it in the signed-in browser, and confirm 2× playback is still active.
- **The player ended but requests still fail**: reload, play again at 2×, export a fresh HAR, and reuse already validated temporary segments when safe.
- **Normal download returns 401/403**: do not guess endpoints or expose credentials; return to current browser/network evidence.

## Completion

Report the clickable output path, verified duration, codecs, resolution, and file size. If reconstructed from streamed tracks, explain that the result is a remux and may not be byte-identical to Microsoft's source container.
