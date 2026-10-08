import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const fixture = () => JSON.parse(readFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/commitlint-golden.json', import.meta.url)));
test('empty upstream lint success survives independent parse failure', () => {
 const empty = fixture().cases.find(c => c.message === '');
 assert.equal(empty.upstreamValid, true);
 assert.equal(empty.valid, false);
 assert.equal(empty.policyException, 'empty-message-policy');
 assert.deepEqual(empty.errors, []);
 assert.equal(empty.lintError, null);
 assert.match(empty.parseError, /Expected a raw commit/);
});
test('newline-only upstream lint failures remain authoritative', () => {
 for (const message of ['\n', '\n\n']) {
 const c = fixture().cases.find(c => c.message === message);
 assert.equal(c.upstreamValid, false);
 assert.equal(c.valid, false);
 assert.match(c.lintError, /Expected a raw commit/);
 assert.deepEqual(c.errors, ['parse-error']);
 assert.equal(c.policyException, null);
 }
});

test('86 upstream assertions retain actual calls, expected results and provenance', () => {
 const data = JSON.parse(readFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/upstream-rules.json', import.meta.url)));
 assert.equal(data.cases.length, 86);
 assert.equal(data.sourceFiles.length, 9);
 assert.equal(new Set(data.cases.map(c=>c.id)).size,86);
 assert.equal(data.upstream.sourceSHA,'95d40569d2592bf9719bc27da2d51fc6b801e4ef');
 assert.equal(data.upstream.tagObjectSHA,'384960a8f54c2f0524c8ea5e89d5a919274239bd');
 assert.match(data.license,/Copyright \(c\) 2016 - present Mario Nebl/);
 for(const c of data.cases) {
  assert.equal(typeof c.expected.valid,'boolean');
  assert.equal(c.parserPreset,'angular');
  assert.match(c.source.sha256,/^[a-f0-9]{64}$/);
  assert.ok(c.source.testLine<c.source.assertionLine);
 }
 const footer=data.cases.filter(c=>c.rule==='footer-max-line-length');
 assert.equal(footer[4].message,footer[2].message);
 assert.equal(footer[5].message,footer[3].message);
});

test('whole-policy inputs are explicitly distinct from individual rule assertions', () => {
 const data=JSON.parse(readFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/upstream-input-policy.json',import.meta.url)));
 assert.equal(data.cases.length,86);
 assert.ok(data.cases.every(c=>c.provenanceKind==='upstream-input-policy-oracle'));
 const c=data.cases.find(c=>c.message==='(scope):');
 assert.equal(c.valid,false);
 // The first individual type-empty assertion for this input expects true.
 assert.ok(c.errors.includes('type-empty'));
});

test('regeneration refuses changed source bytes and reviewed binding tables', async () => {
 const {validateUpstreamSources}=await import('./upstream-source.mjs');
 const {mkdtempSync,cpSync,rmSync,appendFileSync}=await import('node:fs');
 const {tmpdir}=await import('node:os');
 const {join}=await import('node:path');
 const source=new URL('./upstream-v21.2.3/',import.meta.url);
 const directory=mkdtempSync(join(tmpdir(),'commitlint-source-test-'));
 try {
  const copy=join(directory,'source');cpSync(source,copy,{recursive:true});
  assert.equal(validateUpstreamSources(copy).cases.length,86);
  appendFileSync(join(copy,'header-max-length.test.ts'),'\n// corruption\n');
  assert.throws(()=>validateUpstreamSources(copy),/source hash/);
  cpSync(source,copy,{recursive:true});
  appendFileSync(join(copy,'assertions.json'),' ');
  assert.throws(()=>validateUpstreamSources(copy),/reviewed argument binding manifest changed/);
 } finally {rmSync(directory,{recursive:true,force:true});}
});

test('live pinned upstream lint and parsing retain independent empty-input outcomes', async () => {
 const {default:load}=await import('@commitlint/load');
 const {fileURLToPath}=await import('node:url');
 const {dirname,join}=await import('node:path');
 const {evaluate}=await import('./evaluate.mjs');
 const here=dirname(fileURLToPath(import.meta.url));
 const config=await load({}, {file:join(here,'commitlint.config.mjs'),cwd:here});
 for(const message of ['', '\n', '\n\n']) {
  const actual=await evaluate(message,config);
  const expected=fixture().cases.find(c=>c.message===message);
  for(const key of ['valid','upstreamValid','policyException','lintError','parseError','errors','warnings','parsed']) {
   assert.deepEqual(actual[key],expected[key],`${JSON.stringify(message)} ${key}`);
  }
 }
});
