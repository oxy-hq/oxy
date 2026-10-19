// Types for the generator, for the test that imports it. The script is plain
// JavaScript so `pnpm gen:manifest-schema` needs no build.

import type { ManifestSchemaDocument } from "../src/custom-app/manifest-schema";

export const SCHEMA_PATH: string;
export const SCHEMA_ID: string;
export function generateManifestSchema(): ManifestSchemaDocument;
export function renderManifestSchema(): string;
