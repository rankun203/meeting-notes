# Repository workflow

- This is a single-maintainer repository. Work directly on the default branch (`master`).
- Do not create feature branches or pull requests unless the user explicitly asks for one.
- Use Conventional Commits format for commit messages (for example, `feat: add export support` or `fix(audio): handle device loss`).
- Before committing, inspect the complete diff and run the relevant checks.
- For standalone or temporary Python scripts, use `uv run --no-project` and add required third-party packages with `--with` (for example, `uv run --no-project --with httpx script.py`). Use the repository's normal project environment for project-owned Python code.
