---
id: BIT-US-0185
type: story
title: Fenced code block in a nested block renders its text outside the code box, without line numbers
status: done
priority: high
parent: BIT-EP-0027
milestone: BIT-M-0011
author: mcp
labels: [bug, bitacora-app, editor, code-block]
estimate: 3
created: 2026-10-09T11:23:31Z
updated: 2026-10-09T12:12:48Z
started: 2026-10-09T11:35:41Z
closed: 2026-10-09T12:12:48Z
---

## Description
Owner screenshot (2026-10-09). Source (journal 2026_10_09.md, tab-indented, valid Logseq):
```
	- En la config de Nginx
		- ```
		  fastcgi_param HTTPS on;
		  ```
```
Rendered: the bullet line shows a literal "```", the code text `fastcgi_param HTTPS on;` is drawn as plain text below it, and an empty code box (with "Copiar código") appears underneath. Expected: the code inside the box, in place of the block content, monospace, with line numbers in a gutter (like Logseq's CodeMirror view), fences hidden when not editing. Check continuation-line indentation (`  ` after tabs) stripping and fences without a language.

## Acceptance Criteria
- Code block content renders inside the code box, with line numbers; no literal fences outside editing.
- Round-trip unchanged (user files sacred); tests with this exact fixture (nested, tab indent, no language) and with a language.
