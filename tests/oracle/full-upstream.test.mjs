import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const root=new URL('./upstream-all-v21.2.3/',import.meta.url);
test('actual upstream callbacks reject a mutated Boolean expectation',async()=>{
 const {runUpstreamSuite}=await import('./full-upstream-harness.mjs');
 const path='@commitlint/rules/src/type-empty.test.ts';
 const original=readFileSync(new URL(path,root),'utf8');
 const changed=original.replace('const expected = true;','const expected = false;');
 assert.notEqual(changed,original);
 await assert.rejects(()=>runUpstreamSuite({sourceOverrides:{[path]:changed}}),/Expected values|strictly equal|type-empty/);
});
test('actual upstream callbacks reject a mutated message expectation',async()=>{
 const {runUpstreamSuite}=await import('./full-upstream-harness.mjs');
 const path='@commitlint/rules/src/subject-case.test.ts';
 const original=readFileSync(new URL(path,root),'utf8');
 const changed=original.replace('"subject must not be sentence-case"','"deliberately incorrect diagnostic"');
 assert.notEqual(changed,original);
 await assert.rejects(()=>runUpstreamSuite({sourceOverrides:{[path]:changed}}),/Expected values|strictly equal|subject-case/);
});

test('all original tests, source metadata, and observed outcomes remain separate',async()=>{
 const data=JSON.parse(readFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/full-upstream-rules.json',import.meta.url)));
 assert.equal(data.schema,3);
 assert.equal(data.coverage.functionalTests,532);
 assert.equal(data.coverage.metaTests,3);
 assert.equal(data.coverage.ruleCalls,532);
 assert.equal(data.coverage.assertions,632);
 assert.equal(data.coverage.ruleNames.length,38);
 assert.equal(data.tests.length,535);
 assert.equal(data.cases.length,532);
 assert.ok(data.tests.every(t=>t.passed&&t.assertions.length>0));
 assert.ok(data.cases.every(c=>c.source.line>0&&c.source.testLine>0&&c.source.sha256.length===64));
 assert.equal(data.execution.node,'v26.8.2');
 assert.equal(data.execution.unicode,'17.0');
 assert.equal(data.execution.bunTranspiler,'1.3.6');
 const messageOnly=data.cases.find(c=>c.source.test==='should use expected message with "never"');
 assert.ok(messageOnly);
 assert.deepEqual(messageOnly.assertions.map(a=>a.matcher),['toContain']);
 assert.equal(typeof messageOnly.outcome.valid,'boolean');
 assert.notEqual(messageOnly.assertions[0].expected.value,messageOnly.outcome.valid);
 const refs=data.cases.find(c=>c.rule==='references-empty'&&c.message==='bar REF-1234');
 assert.equal(refs.parsed.references[0].prefix,'REF-');
 assert.deepEqual(refs.parserOptions.value.issuePrefixes,['REF-']);
});

test('unknown original matchers fail generation rather than silently skipping assertions',async()=>{
 const {runUpstreamSuite}=await import('./full-upstream-harness.mjs');
 const path='@commitlint/rules/src/type-empty.test.ts';
 const original=readFileSync(new URL(path,root),'utf8');
 const changed=original.replace('expect(actual).toEqual(expected);','expect(actual).toInventedMatcher(expected);');
 await assert.rejects(()=>runUpstreamSuite({sourceOverrides:{[path]:changed}}),/unsupported upstream matcher/);
});

test('missing Git refuses the reference run and restores caller environment',async()=>{
 const {runUpstreamSuite}=await import('./full-upstream-harness.mjs');
 const cwd=process.cwd(),home=process.env.HOME,path=process.env.PATH;
 await assert.rejects(()=>runUpstreamSuite({gitExecutable:'/nonexistent-commitlint-test-git'}),/Git unavailable/);
 assert.equal(process.cwd(),cwd);
 assert.equal(process.env.HOME,home);
 assert.equal(process.env.PATH,path);
});
