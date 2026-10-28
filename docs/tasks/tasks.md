---
title: Tasks
part_of: '[Twig Export](/README.md)'
description: Deferred work in twig-export, one file each, every one with a done state
contents:
- '[A number at the start of a line becomes a list item](/docs/tasks/a-number-at-a-line-start-becomes-a-list.md)'
- '[A tile keeps a frame only svg reads](/docs/tasks/a-tile-keeps-a-frame-only-svg-reads.md)'
---

# Tasks

Deferred work in twig-export, one file each. A bug is a task with a repro.
Each carries a `status` (`open`, `in-progress`, `done` or `dropped`), and
closing one is setting it and naming the commit that resolved it, not
deleting the file.
