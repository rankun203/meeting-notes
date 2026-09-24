-- Target only the installer window; never change Finder's global view preferences.
on run argv
    set installerFolder to POSIX file (item 1 of argv) as alias
    set backgroundFile to POSIX file ((item 1 of argv) & "/../installer-background.tiff") as alias
    tell application "Finder"
        -- Reopening also clears a stale, alphabetically arranged installer window.
        repeat with existingWindow in (get Finder windows)
            try
                -- Virtual locations such as Recents cannot be coerced to an alias.
                if (target of existingWindow as alias) is installerFolder then close existingWindow
            end try
        end repeat
        set installerWindow to make new Finder window to installerFolder
        -- Let Finder restore saved folder options before applying our layout.
        delay 0.5
        set current view of installerWindow to icon view
        set toolbar visible of installerWindow to false
        set bounds of installerWindow to {200, 150, 840, 654}
        set icon size of icon view options of installerWindow to 96
        set arrangement of icon view options of installerWindow to snap to grid
        set background picture of icon view options of installerWindow to backgroundFile
        set position of item "Gday Meetings.app" of installerFolder to {100, 120}
        set position of item "Applications" of installerFolder to {280, 120}
        -- Reopen to render the newly saved background instead of Finder's cached image.
        close installerWindow
        open installerFolder
        set installerWindow to container window of installerFolder
        delay 0.5
        set index of installerWindow to 1
        activate
        set selection to {}
        select item "Gday Meetings.app" of installerFolder
    end tell
end run
