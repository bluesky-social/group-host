import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

// The pages content.mjs copies from docs/.
export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() })
};
