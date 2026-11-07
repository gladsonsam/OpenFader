import { getCollection, type CollectionEntry } from 'astro:content';

export type Doc = CollectionEntry<'docs'>;

export const GROUPS = ['Start', 'Mixer', 'Recording', 'App'] as const;

export async function sortedDocs(): Promise<Doc[]> {
  const docs = await getCollection('docs');
  return docs.sort((a, b) => a.data.order - b.data.order);
}

export const docHref = (doc: Doc) => `/docs/${doc.id}`;
