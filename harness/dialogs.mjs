// A dialog's title is a few fixed words ("Remove this member?", "Invite
// someone"), never words built around a name, a handle or anything
// someone typed: those are as long as people make them, and a title
// holding one wraps over lines or runs past the dialog's edge. Which
// thing the dialog is about is said in its body, where it wraps as text.
//
//   node harness/dialogs.mjs
//
// Reads every component and page under web/src, and prints each dialog
// (a tag whose name ends in Dialog) whose title is a template literal
// with a `${…}` in it, or strings joined with `+`, as `file:line: title`,
// failing if any. A title passed whole (`title={title}`, as ConfirmDialog
// hands its own to Dialog, or a choice between fixed words) is let be:
// what reaches it is the caller's, and checked where the caller writes it.

import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const src = join(root, 'web/src')

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.svelte') ? [join(d, e.name)] : []
  )

/** The expression in `text` from `at`, just past its opening `{`, to its matching `}`. */
function expression(text, at) {
  let depth = 1
  let quote = null
  for (let i = at; i < text.length; i++) {
    const c = text[i]
    if (quote) {
      if (c === '\\') i++
      else if (c === quote) quote = null
      else if (quote === '`' && c === '$' && text[i + 1] === '{') {
        // Inside a template's `${`, braces count again until it closes.
        const inner = expression(text, i + 2)
        i += 2 + inner.length
      }
      continue
    }
    if (c === '"' || c === "'" || c === '`') quote = c
    else if (c === '{') depth++
    else if (c === '}' && --depth === 0) return text.slice(at, i)
  }
  return text.slice(at)
}

const problems = []
for (const file of files(src)) {
  const text = readFileSync(file, 'utf8')
  // Each dialog's opening tag, as far as its title.
  for (const tag of text.matchAll(/<(\w*Dialog)\b[^<]*?\btitle=\{/g)) {
    const at = tag.index + tag[0].length
    const title = expression(text, at)
    const built = /`[^`]*\$\{/.test(title) || /['"`]\s*\+|\+\s*['"`]/.test(title)
    if (!built) continue
    const line = text.slice(0, at).split('\n').length
    problems.push(`${relative(root, file)}:${line}: <${tag[1]} title={${title}}>`)
  }
}

if (problems.length) {
  console.log(problems.join('\n'))
  console.log(`dialogs: ${problems.length} titles built around a value; say which thing in the body (harness/dialogs.mjs)`)
  process.exit(1)
}
console.log("dialogs: every title is the dialog's own words")
