import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Build-time only. The website and developer tools read the same checked catalogue.
const script = fileURLToPath(new URL('../../../scripts/mixer-lab/integration_matrix.py', import.meta.url));
export interface MatrixModel { id: string; brand: string; name: string; family: string; scope: string }
export interface MatrixFeature { id: string; label: string; scope: string }
export interface MatrixCell { id: string; model: string; feature: string; state: string; label: string }
export const matrix: { models: MatrixModel[]; features: MatrixFeature[]; cells: MatrixCell[] } = JSON.parse(
  execFileSync('python3', [script, 'report', '--format', 'public-json'], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 }),
);
