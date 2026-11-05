import { unified } from '@astrojs/markdown-remark'
import starlight from '@astrojs/starlight'
import { defineConfig } from 'astro/config'
import { REPO, remarkLinks } from './plugins.mjs'
import { remarkReference } from './reference.mjs'

/** Where the site is served: a private repository's site is at its own host's root. */
const base = '/'

// The sidebar follows docs/README.md: design, then reference, then development.
const page = (label, slug) => ({ label, slug })

export default defineConfig({
  base,
  markdown: { processor: unified({ remarkPlugins: [[remarkLinks, { base }], remarkReference] }) },
  integrations: [
    starlight({
      title: 'group-host docs',
      description: 'A group host for atproto: design, reference and development.',
      customCss: ['./src/theme.css'],
      components: { SiteTitle: './src/SiteTitle.astro', ThemeSelect: './src/ThemeSelect.astro' },
      social: [{ icon: 'seti:git', label: 'The repository', href: REPO }],
      lastUpdated: false,
      // Code as the console shows it: a plain block, GitHub's colours.
      expressiveCode: {
        themes: ['github-dark', 'github-light'],
        defaultProps: { frame: 'none' },
        styleOverrides: {
          borderRadius: 'var(--radius-md)',
          borderColor: 'var(--line)',
          codeBackground: 'var(--soft)',
          codeFontFamily: 'var(--font-mono)',
          codeFontSize: 'var(--text-sm)',
          uiFontFamily: 'var(--font-sans)',
          frames: { frameBoxShadowCssValue: 'none' },
        },
      },
      pagination: true,
      sidebar: [
        page('Overview', ''),
        {
          label: 'Design',
          items: [
            page('Overview', 'design'),
            page('Spaces', 'design/spaces'),
            page('Groups', 'design/groups'),
            page('Membership', 'design/membership'),
            page('Signing in', 'design/signing-in'),
            page('Moderation', 'design/moderation'),
            page('Sites', 'design/sites'),
            page('MCP', 'design/mcp'),
            page('Recovery', 'design/recovery'),
            page('Moving', 'design/moving'),
          ],
        },
        {
          label: 'Reference',
          items: [
            page('HTTP and XRPC API', 'reference/api'),
            page('MCP tools', 'reference/mcp'),
            page('Commands', 'reference/cli'),
            page('Configuration', 'reference/config'),
          ],
        },
        { label: 'Contributing', items: [page('Development', 'development')] },
      ],
    }),
  ],
})
