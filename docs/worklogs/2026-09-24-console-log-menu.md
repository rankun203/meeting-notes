---
date: 2026-09-24
title: Open the current client log in Console
status: completed
---

## Problem

Show Logs opened the log directory in Finder instead of the current log in Console.app.

## Implemented solution

The menu now resolves the newest existing daily client log in the configured log directory and opens it with `/usr/bin/open -b com.apple.Console`. Browser opening retains the default application behavior.

## Reasoning

Select an existing daily file so the command also works before the first log event following midnight. Use Console's bundle identifier to explicitly select the viewer, regardless of default `.log` associations. Application opening remains off the UI thread.

## Technical debt

None.

## Notes

Validation: client compilation and existing logging tests. The running app is not stopped or overwritten; the menu change takes effect after installing the rebuilt app and restarting.
