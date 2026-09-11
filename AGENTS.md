# Repository workflow

- This is a single-maintainer repository. Work directly on the default branch (`master`).
- Do not create feature branches or pull requests unless the user explicitly asks for one.
- Use Conventional Commits format for commit messages (for example, `feat: add export support` or `fix(audio): handle device loss`).
- Before committing, inspect the complete diff and run the relevant checks.
- For standalone or temporary Python scripts, use `uv run --no-project` and add required third-party packages with `--with` (for example, `uv run --no-project --with httpx script.py`). Use the repository's normal project environment for project-owned Python code.

## Worklogs

When a task changes repository code, create or update a concise worklog under
`docs/worklogs/` named `YYYY-MM-DD-short-name.md`. Use a YAML metadata header
and record progress, key decisions, results, and blockers as the task evolves,
without repeating information. Do not create a worklog for a task that makes no
repository code changes unless the user explicitly requests one.

- **Problem:** what was wrong or missing.
- **Implemented solution:** what changed and where.
- **Reasoning:** why this approach was chosen, including important tradeoffs or
  rejected alternatives.
- **Technical debt:** state whether the sub-task introduced or retained any
  shortcut, deferred compromise, compatibility bridge, temporary duplication,
  or database/schema debt. For each applicable item, record why it was
  accepted, its operational or maintenance consequence, and the concrete
  future remediation. Write `None` when the assessment found no added or
  retained technical debt.
- **Notes:** optional related context worth preserving, such as compatibility,
  migration, validation, release, or remaining-blocker details.
