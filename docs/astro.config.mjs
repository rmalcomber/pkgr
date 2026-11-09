import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import starlightLinksValidator from 'starlight-links-validator'
import { unified } from '@astrojs/markdown-remark'
import { site as S } from './src/site.config.mjs'
import { tokens } from './src/plugins/tokens.mjs'

export default defineConfig({
  site: S.site,
  base: S.base,
  markdown: { processor: unified({ remarkPlugins: [[tokens, { base: S.base }]] }) },
  integrations: [
    starlight({
      title: S.name,
      description: S.tagline,
      logo: { src: './src/assets/logo.svg', alt: S.name },
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: S.repoUrl }],
      editLink: { baseUrl: `${S.repoUrl}/edit/main/docs/` },
      customCss: ['./src/styles/theme.css'],
      // Dark is the default; visitors can still switch.
      head: [
        { tag: 'script', content: "try{if(!localStorage.getItem('starlight-theme')){localStorage.setItem('starlight-theme','dark');document.documentElement.dataset.theme='dark'}}catch(e){}" },
      ],
      plugins: [starlightLinksValidator({ errorOnRelativeLinks: false, errorOnLocalLinks: false })],
      sidebar: [
        { label: 'Getting started', items: [{ autogenerate: { directory: 'getting-started' } }] },
        { label: 'Guides', items: [{ autogenerate: { directory: 'guides' } }] },
        { label: 'Reference', items: [{ autogenerate: { directory: 'reference' } }] },
      ],
    }),
  ],
})
