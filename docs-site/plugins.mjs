// The docs' links, written to read in the repository's pages, made the
// site's: a link between pages goes to the page on the site, and a link
// out of docs/ to the file in the repository.

import { dirname, join, posix, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const content = join(here, 'src/content/docs')
/** Where a file outside docs/ is read: the repository's pages on knotless.dev. */
export const REPO = 'https://knotless.dev/chadtmiller.com/group-host'

export const walk = (node, visit) => {
  visit(node)
  for (const child of node.children ?? []) walk(child, visit)
}

/** The page a link in `page`, a path under docs/, lands on, as a URL. */
function resolve(page, url, base) {
  if (/^[a-z]+:|^#|^\//.test(url)) return url
  const [path, anchor] = url.split('#')
  const target = posix.normalize(posix.join('docs', posix.dirname(page), path))
  const hash = anchor ? `#${anchor}` : ''
  if (target.startsWith('docs/') && target.endsWith('.md')) {
    const slug = target.slice('docs/'.length).replace(/\.md$/, '').replace(/(^|\/)README$/, '')
    return `${base}${slug ? `${slug.toLowerCase()}/` : ''}${hash}`
  }
  // A directory has no extension: lexicons/fyi/opensocial, web.
  const kind = posix.extname(target) ? 'blob' : 'tree'
  return `${REPO}/${kind}/main/${target}${hash}`
}

/** Links between pages go to the site's pages; links out of docs/, to the repository. */
export function remarkLinks({ base }) {
  return (tree, file) => {
    const page = relative(content, file.path).replace(/(^|\/)index\.md$/, '$1README.md')
    walk(tree, (node) => {
      if (node.type === 'link' || node.type === 'definition') node.url = resolve(page, node.url, base)
    })
  }
}
