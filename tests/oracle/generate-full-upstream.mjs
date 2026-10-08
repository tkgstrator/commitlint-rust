import {writeFileSync} from 'node:fs';
import {runUpstreamSuite} from './full-upstream-harness.mjs';
const fixture=await runUpstreamSuite();
writeFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/full-upstream-rules.json',import.meta.url),JSON.stringify(fixture,null,2)+'\n');
console.log(JSON.stringify({schema:fixture.schema,coverage:fixture.coverage,execution:fixture.execution},null,2));
