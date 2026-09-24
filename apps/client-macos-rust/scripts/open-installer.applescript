-- Target only this new window; never change Finder's global view preferences.
on run argv
    set installerFolder to POSIX file (item 1 of argv) as alias
    tell application "Finder"
        set installerWindow to make new Finder window to installerFolder
        set current view of installerWindow to icon view
        set toolbar visible of installerWindow to false
        set bounds of installerWindow to {200, 150, 840, 510}
        set icon size of icon view options of installerWindow to 96
        set arrangement of icon view options of installerWindow to arranged by name
        activate
    end tell
end run
