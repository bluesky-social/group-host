// The docs: every relative link reaches a file, and every `#anchor` a
// heading in it, as the repository's pages and the docs site slug them.
//
//   node harness/docs.mjs
//
// Prints each problem as `file:line: what`, failing if any.

import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.md') ? [join(d, e.name)] : [],
  )
const docs = ['README.md', 'AGENTS.md'].map((f) => join(root, f)).concat(files(join(root, 'docs')))

// The lines outside fences, with the line each is.
const split = (text) => {
  const prose = []
  let fenced = false
  text.split('\n').forEach((line, i) => {
    if (line.startsWith('```')) fenced = !fenced
    else if (!fenced) prose.push([i + 1, line])
  })
  return { prose }
}

const slugged = new Map()
const slugs = (file) => {
  if (!slugged.has(file)) {
    const seen = new Map()
    const out = new Set()
    for (const [, line] of split(readFileSync(file, 'utf8')).prose) {
      const m = line.match(/^#{1,6}\s+(.*)/)
      if (!m) continue
      const slug = m[1].trim().toLowerCase().replace(/[^\p{L}\p{N}_\- ]/gu, '').replaceAll(' ', '-')
      const n = seen.get(slug) ?? 0
      seen.set(slug, n + 1)
      out.add(n ? `${slug}-${n}` : slug)
    }
    slugged.set(file, out)
  }
  return slugged.get(file)
}

const problems = []
for (const file of docs) {
  const name = relative(root, file)
  const { prose } = split(readFileSync(file, 'utf8'))
  for (const [n, line] of prose) {
    for (const [, url] of line.replace(/`[^`]*`/g, '').matchAll(/\]\(([^)\s]+)\)/g)) {
      if (/^[a-z]+:/.test(url)) continue
      const [path, anchor] = url.split('#')
      const target = path ? resolve(dirname(file), path) : file
      if (!existsSync(target)) problems.push(`${name}:${n}: no ${path}`)
      else if (anchor && target.endsWith('.md') && !slugs(target).has(anchor))
        problems.push(`${name}:${n}: no heading #${anchor} in ${relative(root, target)}`)
    }
  }
}

for (const p of problems) console.log(p)
console.log(`${docs.length} pages, ${problems.length} problems`)
process.exit(problems.length ? 1 : 0)
