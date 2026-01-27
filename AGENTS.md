# AGENTS

This document outlines the rules for conventional commits in the Bottle ORM project, along with references to our contributing guidelines and code of conduct.

---

## Conventional Commits

We follow the [Conventional Commits](https://conventionalcommits.org/) specification to ensure consistent and meaningful commit messages. This helps in automating versioning, changelog generation, and understanding the history of changes.

### Format

A conventional commit message follows this structure:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

- **type**: Describes the kind of change (see below).
- **scope**: Optional. Specifies the module or area affected (e.g., `auth`, `api`).
- **description**: A short, imperative description of the change.
- **body**: Optional. Provides additional context or details.
- **footer**: Optional. Used for breaking changes, issue references, etc.

### Types

- `feat`: Introduces a new feature.
- `fix`: Fixes a bug.
- `docs`: Changes to documentation only.
- `style`: Changes that do not affect code meaning (e.g., formatting, white-space).
- `refactor`: Code changes that neither fix a bug nor add a feature.
- `test`: Adds or corrects tests.
- `chore`: Changes to build processes, tools, or auxiliary files.

### Examples

- `feat: add user authentication`
- `fix(api): resolve null pointer exception in endpoint`
- `docs: update README with installation instructions`
- `refactor(core): simplify database connection logic`

### Breaking Changes

If a commit introduces a breaking change, include `BREAKING CHANGE:` in the footer, followed by a description.

### Reverting Commits

Use `revert:` type for commits that revert previous changes, referencing the original commit.

---

## Pull Request Descriptions

When submitting a pull request, provide a detailed description following this structure to ensure clarity and facilitate review.

### Problem Description

Describe the issue or feature being addressed. Include any relevant context, such as error messages, user stories, or requirements.

### Root Cause

If applicable, explain the underlying cause of the problem. This helps reviewers understand why the issue exists.

### Solution

Outline the proposed fix or implementation. Provide a high-level overview of how the changes address the problem.

### Changes Made

List the specific files, functions, or components modified. Use bullet points for clarity, and reference line numbers if possible.

### Testing

Describe how the changes were tested. Include unit tests, integration tests, manual testing, or any other validation methods used.

### Impact

Discuss the effects of the changes, including:

- **Scope:** What parts of the codebase are affected?
- **Backward Compatibility:** Does this introduce breaking changes?
- **Performance:** Any performance implications?
- **Dependencies:** New dependencies or changes to existing ones?

This structure ensures that pull requests are comprehensive and easy to review, aligning with our commitment to high-quality contributions.

---

For more information on contributing, including how to submit pull requests and report issues, please refer to [CONTRIBUTING.md](CONTRIBUTING.md).

All contributors are expected to adhere to our [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) to maintain a respectful and inclusive community.
