// The web components' layers, atomic design's: atoms, molecules built of
// atoms, organisms of those, and templates laying organisms out. A
// component uses its own layer and those below it, never one above, so
// each layer can be understood, and changed, knowing only what is under it.
//
//   node harness/layers.mjs
//
// Prints each import that reaches up as `file:line: what`, failing if any.

import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const lib = join(root, 'web/src/lib')
const layers = ['atoms', 'molecules', 'organisms', 'templates']

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(join(d, e.name)) : [join(d, e.name)]))

const problems = []
for (const [rank, layer] of layers.entries()) {
  for (const file of files(join(lib, layer))) {
    const lines = readFileSync(file, 'utf8').split('\n')
    lines.forEach((line, i) => {
      for (const [, path] of line.matchAll(/(?:from|import)\s+['"]([^'"]+)['"]/g)) {
        const target = path.startsWith('#lib/')
          ? join(lib, path.slice('#lib/'.length))
          : path.startsWith('.')
            ? resolve(dirname(file), path)
            : null
        const used = target && layers.indexOf(relative(lib, target).split('/')[0])
        if (used > rank)
          problems.push(`${relative(root, file)}:${i + 1}: ${layer} use only ${layers.slice(0, rank + 1).join(', ')}, not ${layers[used]}`)
      }
    })
  }
}

for (const p of problems) console.log(p)
if (problems.length) process.exit(1)
console.log(`web: ${layers.join(' < ')}, each using only its own layer and those below`)
