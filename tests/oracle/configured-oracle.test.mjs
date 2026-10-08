import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const fixture=()=>JSON.parse(readFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/configured-upstream.json',import.meta.url)));
const row=id=>fixture().cases.find(c=>c.id===id);
test('actual JSON loader resolves preset limits and whole-tuple overrides',()=>{
 assert.equal(fixture().cases.length,111);
 assert.equal(row('preset-100-pass').upstream.result.valid,true);
 assert.equal(row('preset-100-fail').upstream.result.valid,false);
 assert.equal(row('preset-128-override-pass').upstream.result.valid,true);
 assert.equal(row('preset-128-override-fail').upstream.result.valid,false);
 assert.deepEqual(row('preset-override-disabled-shorthand').upstream.loaded.rules['header-max-length'],[0]);
 assert.deepEqual(row('preset-override-disabled-full-tuple').upstream.loaded.rules['header-max-length'],[0,'always',{unused:true}]);
});
test('configured diagnostics retain severity and original rule insertion order',()=>{
 const normal=row('severity-warning-and-error-order').upstream.result;
 assert.deepEqual(normal.errors.map(d=>d.name),['scope-empty','header-max-length','type-empty']);
 assert.deepEqual(normal.warnings.map(d=>d.name),['subject-empty']);
 assert.ok(normal.errors.every(d=>d.level===2));assert.ok(normal.warnings.every(d=>d.level===1));
 assert.deepEqual(row('severity-reversed-error-order').upstream.result.errors.map(d=>d.name),['type-empty','header-max-length','scope-empty']);
 for(const literal of ['2.0','2e0','0.0','0e0'])assert.match(row(`severity-number-${literal}`).configText,new RegExp(literal.replace('.','\\.')));
});
test('parser object names remain metadata instead of implicitly loading modules',()=>{
 const c=row('parser-object-conventional-changelog-conventionalcommits');
 assert.deepEqual(c.upstream.loaded.parserOpts,{});
 assert.equal(c.upstream.parsed.type,null);
 assert.equal(c.upstream.result.valid,false);
 assert.equal(row('parser-string-conventional-changelog-conventionalcommits').upstream.parsed.type,'feat');
 assert.equal(row('parser-options-only-overrides-conventional').upstream.parsed.type,null);
 for(const id of ['parser-empty-object-overrides-conventional','parser-name-only-overrides-conventional','parser-angular-name-only-keeps-inherited-conventional'])assert.equal(row(id).upstream.parsed.type,'feat');
});
test('native profile differences preserve original upstream acceptance evidence',()=>{
 for(const id of ['config-null','parser-options-not-object','plugins-empty','unused-native-value']) {
  const c=row(id);assert.ok(c);
  assert.equal(c.nativeExpected,'config-error');assert.equal(typeof c.deviation,'string');
  assert.equal(c.upstream.result.valid,true);
 }
 const unknown=fixture().cases.find(c=>c.deviation==='validate-config-before-shortcut'&&c.message==='');
 assert.ok(unknown);assert.equal(unknown.upstream.result.valid,true);assert.equal(unknown.nativeExpected,'config-error');
 const empty=fixture().cases.find(c=>c.message===''&&c.category==='supported');
 assert.equal(empty.upstream.result.valid,true);
 assert.equal(empty.upstream.loadError,null);assert.equal(empty.upstream.lintError,null);
 assert.match(empty.upstream.parseError,/Expected a raw commit/);
 const newline=fixture().cases.find(c=>c.message==='\n'&&c.category==='supported');
 assert.equal(newline.nativeExpected,'lint-error');assert.match(newline.upstream.lintError,/Expected a raw commit/);
});
