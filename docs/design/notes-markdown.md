---
title: Markdown notes with images and timeline links
date: 2026-09-26
status: proposed
scope: swift-app-notes
---

# Markdown notes with images and timeline links

This proposal covers the Notes tab in the Swift macOS client. Nothing described here is implemented yet. It needs approval of the recommended direction and answers to the [open questions](#open-questions) before implementation starts.

## Goals

1. Paste or drag images into notes, and read them inline.
2. Store notes as Markdown.
3. Link each note to a time on the recording timeline. Command-click on a word plays from that time, as clicking a transcript time does.
4. Keep editing native, offline, and buildable with Command Line Tools only.

## Current state

- `Meeting.notes` is a plain `String` in `library.json` (`Core/Models.swift`). Every keystroke calls `MeetingStore.updateMeeting`, which rewrites the whole library file.
- The Notes tab is a SwiftUI `TextEditor` bound to that string (`UI/MeetingDetailView.swift`, `editor(_:binding:)`).
- Transcript times use `playback.play(meeting:files:at:)`. That method seeks if the meeting is already loaded and loads it otherwise. Notes should use the same call.
- The meeting folder (`MeetingStore.directory(for:)`) holds audio and `server-archive.json`. Meetings created with **New Meeting Notes** have no folder until audio is added.
- Notes are also read by library search (`LibraryView`), summaries and chat (`MeetingIntelligence`), Markdown and JSON export (`MeetingStore.exportMeeting`), JSON import, legacy import, and server archive (`ServerArchive`, artifact `notes.md`).
- The server accepts only flat `.json`, `.md`, and `.txt` artifacts (`apps/server/src/server/import-meeting.ts`). It has no place for images.
- Audio is written against a host-clock recording epoch (`AudioCapture.epoch`, `TimedAudioWriter`). `MeetingStore.recordingStartedAt` is set after capture starts, so it can lag the audio timeline slightly.
- `Package.swift` has no package dependencies. Third-party code is limited to checksum-pinned C source archives built offline ([ThirdParty/README.md](../../apps/client-macos-swift/ThirdParty/README.md)).

## Recommendation

Build a focused native editor: an AppKit `NSTextView` using TextKit 2, wrapped for SwiftUI, with Markdown as the saved format. Show Markdown syntax in place with light styling (Bear-style live preview). Show images and time links as native objects in the editor, and write them back as plain Markdown when saving. Add no third-party dependency for the first phases.

### Why this approach

- `NSTextView` provides input methods, spelling, dictation, undo, Find, Services, Writing Tools, and VoiceOver support. Apple's WWDC26 TextKit session recommends starting from the framework text view and adding behavior, rather than building a custom text view ([WWDC26 session 370](https://developer.apple.com/videos/play/wwdc2026/370)).
- A tailored editor makes the gutter, Command-click, and time tracking straightforward. With an existing editor package, these features would require a fork.
- Styling never changes the saved text. A styling mistake is cosmetic and cannot lose data, so a small in-house styler is enough for meeting notes.
- It keeps the build offline with no new dependency process.

### Editor model

The editor keeps an in-memory attributed string. It converts to and from Markdown only at load and save.

| Saved Markdown | In the editor |
| --- | --- |
| Ordinary Markdown text | The same text. Syntax characters such as `**` and `#` stay visible but dimmed. Headings, emphasis, code, quotes, lists, and links are styled. |
| Time marker `<!-- gday:t=12:34.5 -->` | Removed from the text. Stored as a custom attribute on the characters it covers. Shown in the time gutter. |
| Image line `![Whiteboard](assets/4f2a9c1e5b7d3a60.png)` | One attachment character showing the image. The alt text and path are kept in attributes. |

- Fonts are applied in `NSTextStorageDelegate.textStorage(_:didProcessEditing:range:changeInLength:)`, limited to edited paragraphs, so styling never registers undo steps. Colors can use TextKit 2 rendering attributes.
- Do not read `NSTextView.layoutManager`. Reading it permanently switches the view to TextKit 1 compatibility mode ([NSTextView](https://developer.apple.com/documentation/appkit/nstextview)).
- Formatting commands in the **Format** menu: **Bold** (Command-B), **Italic** (Command-I), **Link…** (Command-K). They add or remove Markdown syntax. Return continues lists and task lists. Tab and Shift-Tab indent list items. Clicking a task checkbox toggles `[ ]` and `[x]`.
- External-URL images stay as Markdown text and are never fetched. This keeps notes offline and private.

### Storage

Store notes in the meeting folder:

```text
<library>/<meeting-id>/
  notes.md          Markdown notes with time markers
  assets/           images referenced by notes.md
    4f2a9c1e5b7d3a60.png
  …                 existing audio and archive files
```

- `notes.md` is the saved copy of the notes. `Meeting.notes` stays as an in-memory property so search, summaries, and export keep working. It is no longer saved to `library.json` after migration.
- The folder name `assets/` matches the [TextBundle](http://textbundle.org/spec) layout, so a TextBundle export is a copy plus `info.json`.
- Write with a short debounce (about 0.5 seconds) and immediately when the editor loses focus, the meeting changes, recording stops, or the app quits. Use atomic writes with `0600` permissions, matching `library.json`. Create the meeting folder on first write.
- **Migration:** at library load, for each meeting whose `notes` is non-empty and whose folder lacks `notes.md`, write `notes.md` and then clear the field. Keep `library.json` at version 1; a missing `notes` key already decodes as empty. An older app build would then show empty notes for migrated meetings. See the open questions.
- **External edits:** when the editor opens a meeting, reload `notes.md` if its modification date changed. Phase 4 adds a file watch. If the file changes while there are unsaved edits, keep the app's version and save the external copy as `notes (changed on disk).md`.

### Time markers

A time marker is a Markdown HTML comment:

~~~markdown
## Budget <!-- gday:t=3:05 -->

- Hiring freeze until Q3 <!-- gday:t=3:41.5 -->
- Revisit vendor contract <!-- gday:t=5:12 -->

![Whiteboard](assets/4f2a9c1e5b7d3a60.png) <!-- gday:t=7:02 -->

<!-- gday:t=9:30 -->
```sql
select * from budgets
```
~~~

**Syntax and placement**

- Format: `<!-- gday:t=[h:]mm:ss[.f] -->`. The `gday:` prefix keeps user comments untouched. The time is seconds on the meeting's shared playback timeline.
- A marker times the text before it on the same line, back to the previous marker or the start of the line. Markers always follow text, never start a line. Inside a list item, a line that starts with `<!--` would begin an HTML block and break the list.
- A fenced code block or table gets one marker on its own line before the block, because a comment inside it would become content.
- HTML comments are raw HTML in [CommonMark](https://spec.commonmark.org/0.31.2/#raw-html). Renderers that pass HTML through or sanitize it do not display comments, so other apps show clean notes. Verify GitHub, Obsidian, and VS Code preview during Phase 1.

**Granularity:** one time per line in Phase 1. A line is a paragraph, heading, list item, or image. Command-click on any word plays from its line's time. The format allows several markers per line, so phrase-level times (Phase 4) need no format change.

**Where the time comes from.** When a line first gets text, the editor asks a clock for the current time:

| Situation | Time recorded |
| --- | --- |
| This meeting is recording | Elapsed time on the recording's audio clock (host time minus `AudioCapture` epoch). Fall back to `MeetingStore.recordingDuration` until capture exposes it. |
| This meeting is loaded in the player, playing or paused | The player position (`playback.progress.time`). This matches [Notability](https://support.gingerlabs.com/hc/en-us/articles/206060617-Recording-and-Playing-Audio), which times notes added during playback. |
| No recording and no playback of this meeting | No time. The line has no marker and no gutter label. |

**How times survive editing**

- Typing within a timed line keeps its time. Fixing a typo later does not retime the line.
- Pressing Return in the middle of a line gives both halves the original time. A new empty line gets its time when its first character is typed, not when Return is pressed.
- Cut and paste within the same meeting keeps times, using a private pasteboard type that carries the meeting ID and Markdown with markers. Paste from anywhere else gets the current time. Markers from another meeting are removed.
- Copy also writes plain Markdown without markers, so pasting into other apps gives clean text.
- If an external editor deletes or damages a marker, that line becomes untimed, or the marker shows as ordinary text. Notes are never lost.
- **Set Time to Playback Position** in the context menu retimes the selected lines. This covers notes written with no recording or playback.

### Command-click and the time gutter

- A fixed-width gutter (about 44 pt, always reserved so text never shifts) shows each timed line's time on its first visual line: caption size, monospaced digits, secondary color. It matches the transcript's time buttons: a link style with hover feedback and the tooltip "Play from this point". Lines without a time show nothing.
- Clicking a gutter time or Command-clicking text calls `playback.play(meeting:files:at:)` with the line's time. Holding Command over timed text shows the pointing-hand cursor and highlights that line's gutter time.
- Command-click normally adds a discontiguous selection in `NSTextView`. Inside notes it plays instead. Command-drag keeps the system behavior.
- While recording, playback is blocked (`isPlaybackBlocked`). Gutter times are disabled and Command-click does nothing, as with transcript times.
- **Keyboard:** **Play From Line** (proposed Command-Return) plays from the line containing the insertion point. It appears in the context menu and the playback menu.
- **VoiceOver:** each visible gutter time is a button labeled, for example, "Play from 12 minutes 34 seconds". The text view offers the custom action **Play From Line**. Image attachments are labeled with their alt text.
- Implementation on macOS 14: overlay the gutter in the scroll view's document view. Position labels from `NSTextLayoutManager.enumerateTextLayoutFragments` after viewport layout, and reuse label views for visible lines only. The TextKit APIs from WWDC26 for line numbers and rendering surfaces are for the 2027 releases. They could replace this overlay later behind an availability check.

### Images

- **Input:** paste image data or image files, and drag image files into the editor. The editor registers image drag types, so drops on the text view go to notes. Audio files dropped on it are passed to the existing `AudioFileDrop` import. Dropping images outside the editor is unchanged.
- **Saving:** keep original bytes for PNG, JPEG, HEIC, GIF, and WebP. Convert TIFF and raw pasteboard data (such as screenshots) to PNG. Name each file by the first 8 bytes of its SHA-256 in hex (for example `assets/4f2a9c1e5b7d3a60.png`). Identical images share one file, and name collisions are very unlikely.
- **Size and layout:** each image is its own line. Display width is the smaller of the image's point width and the text width, keeping the aspect ratio. Decode a downsampled image for display with ImageIO (`CGImageSourceCreateThumbnailAtIndex`) and cache it, so large photos stay responsive. Keep the original file. Warn above an agreed size (see open questions).
- **Reading:** double-click opens the image in Quick Look. The context menu offers **Edit Description…** (alt text), **Show in Finder**, and **Copy Image**.
- **Deletion:** removing an image from notes leaves its file so undo works. When the meeting's notes close, and at launch, move files in `assets/` that `notes.md` no longer references to the Trash. Deleting a meeting already moves its folder to the Trash.
- **Other apps:** relative paths display in Obsidian, VS Code, Typora, and GitHub when the meeting folder is opened. External-URL images are left untouched.

### Editing and reading

Use one live-preview editor for both, with no separate edit and read modes. Dimmed syntax stays visible. Hiding syntax away from the insertion point makes text reflow as the insertion point moves, which conflicts with the stable-layout rule in [UI design](../../apps/client-macos-swift/docs/UI_DESIGN.md). Images, times, headings, and lists already read well in this editor.

A reading view that hides syntax can be added in Phase 4 if needed. Because it is not editable, it has no reflow problem. It could reuse the same styler with syntax characters hidden.

### Effects on other features

| Feature | Change |
| --- | --- |
| Library search | Search notes text with markers removed. |
| Summaries and chat | Send notes with markers written as `[12:34]` prefixes so answers can cite times. Do not send images. |
| Markdown export | Remove markers, or write them as `[12:34]` prefixes. Copy referenced images to a folder beside the exported file and rewrite the paths. Offer TextBundle export when notes contain images. |
| JSON export and import | Include `notes` text with markers. Images are not included. See the open questions. |
| Legacy import | Write imported notes to `notes.md`. |
| Server archive | Send `notes.md` with markers. Images need a new server attachment type, uploaded like audio and included in the archive hash. Until then, show "Images in notes aren't archived yet." before archiving. |
| UI Preview | Add a synthetic `notes.md` with times and a generated image to the fixtures. |

## Alternatives considered

| Option | Result | Main reason |
| --- | --- | --- |
| Native `NSTextView` with an in-house styler (recommended) | Chosen | Native behavior, full control over times and gutter, no dependency. |
| Vendor [swift-markdown-engine](https://github.com/nodes-app/swift-markdown-engine) (MarkdownEngine) | Strong fallback | Apache-2.0, macOS 14, Swift 5.9, core target has no dependencies, active (0.13.0 on 2026-09-20). Already supports live styling, image embeds (`EmbeddedImageProvider`), image paste (`onPasteImage`), and task lists. But it is about 19,000 lines, pre-1.0, has wiki-link and header features we don't need, and has no hooks for a time gutter or Command-click. Adopting it means forking and maintaining a pre-1.0 package. Its manifest declares remote dependencies, so an offline build needs a trimmed local manifest. |
| Vendor [swift-markdown](https://github.com/swiftlang/swift-markdown) (cmark-gfm) for parsing | Deferred | Apache-2.0, 0.9.0 on 2026-09-21, spec-correct GFM parsing with source ranges. Needs `swift-cmark` source too. Worth adding if styling edge cases become a problem, such as nested lists or tables. |
| [STTextView](https://github.com/krzyzanowskim/STTextView) | Rejected | GPL-3.0 or commercial license, while this repository is MIT. It targets code editing. |
| SwiftUI `TextEditor` with `AttributedString` | Rejected | Requires macOS 26 ([WWDC25 session 280](https://developer.apple.com/videos/play/wwdc2025/280)); the app supports macOS 14.2. It edits rich text, not Markdown, and SwiftUI text does not embed attachments ([analysis](https://fatbobman.com/en/posts/a-deep-dive-into-swiftui-rich-text-layout)). |
| Foundation `AttributedString(markdown:)` | Rejected for editing | Parses Markdown one way. It has no source ranges and no way to write back to Markdown. SwiftUI does not render its images or block structure ([example](https://blog.eidinger.info/3-surprises-when-using-markdown-in-swiftui)). Usable for a read-only summary view. |
| [MarkdownUI](https://github.com/gonzalezreal/swift-markdown-ui) or [Textual](https://github.com/gonzalezreal/textual) | Rejected | Display only, not editing. MarkdownUI is in maintenance mode; Textual is early (0.x). |
| `WKWebView` with CodeMirror 6, Milkdown, or TipTap | Rejected | Strong editors: [MarkEdit](https://github.com/MarkEdit-app/MarkEdit/wiki/Why-MarkEdit) shows CodeMirror 6 can feel good on macOS. But bundling them needs Node and npm, or a checked-in minified bundle, which breaks the source-only, Command Line Tools build policy. It also needs a JavaScript bridge for times, playback, images (`WKURLSchemeHandler`), focus, and the Space key; a web content process per open editor; and extra work to match native menus, Services, and Writing Tools. |

Other findings:

- TextKit 2 still has bugs in viewport layout, the extra line fragment, and attachments. Some are regressions between macOS versions ([TextKit 2: the promised land](https://blog.krzyzanowskim.com/2025/08/14/textkit-2-the-promised-land), [STTextView bug list](https://github.com/krzyzanowskim/STTextView)). Meeting notes are short, which limits exposure. Test on macOS 14, 15, and 26.
- [Downright](https://github.com/ezzy1630/Downright) (MIT, macOS 14) uses one `NSTextView` with raw Markdown as the saved text and decorations over source ranges. It supports the same basic approach. Its code is a useful reference, not a dependency.

## Phased plan

Each phase ends with `make format-macos`, `make lint-macos`, `make test-macos`, and a worklog. UI checks use UI Preview in Light, Dark, and System appearance at small and large window sizes, following [UI design](../../apps/client-macos-swift/docs/UI_DESIGN.md).

### Phase 1: Markdown notes with times

- `notes.md` storage, migration, debounced atomic save, and change detection when a meeting opens.
- `NSTextView` editor with live styling, Format menu commands, and list continuation.
- Time markers: parse, serialize, the recording and playback clocks, the time gutter, Command-click, **Play From Line**, and VoiceOver actions.
- Search, summaries, export, and archive read notes with markers removed or converted.
- **Tests:** marker round trip (parse then serialize returns the same bytes for valid files; unrelated comments are untouched); time inheritance on typing, splitting lines, and paste; migration from `library.json`; save on quit; clock choice for recording, playback, and neither.
- **Preview:** timed and untimed lines, Command-click and gutter clicks seek the silent player, keyboard-only use, VoiceOver labels, and no layout shift during playback.
- **Estimate:** about 5–7 days.

### Phase 2: Images

- Paste and drag of images, content-hash files in `assets/`, downsampled display, Quick Look, image context menu, and cleanup of unused files.
- Drop routing between the notes editor and `AudioFileDrop`.
- **Tests:** image line round trip, duplicate images, TIFF to PNG, cleanup keeps referenced files and files needed for undo, path validation (no `..` or absolute paths).
- **Preview:** a large synthetic image, window resizing, dark appearance, and selection and deletion of image lines.
- **Estimate:** about 3–4 days.

### Phase 3: Export and archive

- Markdown export with an assets folder, TextBundle export, JSON export behavior for images, and server attachments for images (server change and archive hash).
- **Tests:** exported bundles open in another Markdown app; archive hash changes when an image changes.
- **Estimate:** about 3–5 days, including the server.

### Phase 4: Refinements (optional)

- Phrase-level times: text appended to a line after a pause (for example 15 seconds) gets a new marker.
- A file watch for `notes.md`, a syntax-hiding reading view, tables, and image width control.
- Adopt the 2027 TextKit gutter APIs behind an availability check.

## Risks

- **TextKit 2 bugs:** layout glitches with attachments or long lines. Mitigation: short notes, no custom layout fragments in Phase 1, testing on three macOS versions.
- **Marker loss in other editors:** some editors may reformat or strip comments. The result is untimed lines, not lost notes.
- **Recording clock:** exact alignment needs a read-only elapsed-time accessor on `AudioCapture`. That code is currently being changed by other work, so coordinate before adding the accessor.
- **Styler edge cases:** nested lists and tables may be styled wrong. The saved text is still correct; swift-markdown is the upgrade path.
- **Command-click conflict:** notes lose Command-click discontiguous selection. Command-drag and Option-drag selection are unaffected.

## Open questions

1. **Editor choice:** approve the in-house `NSTextView` editor, or first spend about a day testing MarkdownEngine in UI Preview against the Phase 1 needs?
2. **Dependency policy:** if a Swift package is adopted later (swift-markdown or MarkdownEngine), may it be vendored as a pinned source folder with its license and a trimmed local manifest? The current policy covers only unmodified C archives.
3. **Downgrade:** after migration, older builds show empty notes for migrated meetings. Is that acceptable, or should `library.json` keep a copy of the notes for one release?
4. **Time granularity:** is one time per line enough for Phase 1, with phrase-level times later?
5. **Lead time:** people often write a note a few seconds after hearing it. Should playback start a few seconds early (for example 3 seconds), and should that be a setting?
6. **Shortcut:** is Command-Return right for **Play From Line**?
7. **Image limits:** keep originals of any size, or downscale images above a limit (for example 4096 pixels or 10 MB) when saving?
8. **Archive and JSON export:** block archiving notes with images until the server supports them, or archive text only with a warning? Should JSON export embed images?
9. **Marker text:** is `<!-- gday:t=12:34.5 -->` acceptable in the saved file, or is a shorter or more readable form preferred?
