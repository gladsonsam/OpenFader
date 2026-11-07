import { defineCollection, z } from 'astro:content';
import { glob } from 'astro/loaders';

const docs = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/docs' }),
  schema: z.object({
    title: z.string(),
    group: z.enum(['Start', 'Mixer', 'Recording', 'App']),
    order: z.number(),
    description: z.string(),
  }),
});

export const collections = { docs };
