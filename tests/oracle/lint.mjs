import load from '@commitlint/load';
import lint from '@commitlint/lint';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
export async function checkMessages(records) {
  const config = await load({}, { file: join(here, 'commitlint.config.mjs'), cwd: here });
  let valid = true;
  for (const record of records) {
    const result = await lint(record.message, config.rules, {
      parserOpts: config.parserPreset?.parserOpts,
      plugins: config.plugins,
      defaultIgnores: false,
      ignores: [],
    });
    if (!result.valid) {
      valid = false;
      console.error(`[gh-identity] commitlint rejected ${record.oid}`);
      for (const error of result.errors) console.error(`  ${error.name}: ${error.message}`);
    }
  }
  return valid;
}
