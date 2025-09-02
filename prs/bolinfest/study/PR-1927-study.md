**DOs**
- **Keep README concise:** Present a tight intro, install snippet, and links to docs; move details out of the README.
```md
# Codex CLI

Install: `npm i -g @openai/codex` or `brew install codex`

- Getting Started: ./docs/usage.md
- Configuration: ./docs/config.md
- Contributing: ./docs/contributing.md
```

- **Link to Codex Web clearly:** Distinguish the local CLI from the cloud product with a simple pointer.
```md
If you want the cloud-based agent, Codex [Web], see <https://chatgpt.com/codex>.
```

- **Relocate deep content to /docs:** Put “usage”, “config”, and “contributing” in dedicated files and link them.
```md
docs/
  usage.md
  config.md
  contributing.md
```

- **Use precise product naming:** Prefer “Codex CLI” for the local tool and “Codex [Web]” for the cloud product.
```md
This is the home of the Codex CLI, OpenAI's coding agent that runs locally.
```

- **Provide minimal navigation instead of a giant ToC:** Replace collapsible mega-ToCs with a short “Further reading” block.
```md
## Further Reading
- Usage: ./docs/usage.md
- Config: ./docs/config.md
- Contributing: ./docs/contributing.md
```

**DON’Ts**
- **Don’t label the project as experimental:** Remove callouts or sections that frame the CLI as “experimental”.
```md
> ⚠️ **Experimental**
# ❌ Remove this
```

- **Don’t reintroduce a massive collapsible ToC in README:** Avoid long `<details>` tables of contents in the root README.
```md
<details>
<summary><strong>Table of contents</strong></summary>
<!-- Hundreds of lines… -->
</details>
# ❌ Don’t include this in README
```

- **Don’t duplicate docs content in README:** Keep only summaries in README and link out; don’t paste full guides.
```md
## Configuration (summary)
See full options and examples in ./docs/config.md
# ❌ Don’t inline all flags and examples here
```

- **Don’t blur product naming:** Avoid vague phrasing that conflates CLI and Web.
```md
“This is Codex from OpenAI.”
# ❌ Too vague; specify CLI vs Web
```

- **Don’t add maturity disclaimers or warning banners:** Communicate facts (what it is, how to use it) without status labels.
```md
> Note: Alpha/Beta/Preview
# ❌ Omit maturity labels from README
```