import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const reviewedBindingsSHA256='63d5305861cc01603917e8d974ce5c3337779e7f9e62834ff65979e68d8eedcc';
export function validateUpstreamSources(root) {
 const bytes=readFileSync(join(root,'assertions.json'));
 assert.equal(createHash('sha256').update(bytes).digest('hex'),reviewedBindingsSHA256,'reviewed argument binding manifest changed; inspect original assertions before updating the frozen digest');
 const manifest=JSON.parse(bytes),sourceFiles=new Map();
 assert.equal(manifest.cases.length,86,'exact upstream assertion count');
 assert.equal(manifest.files.length,9,'exact selected source file count');
 for(const file of manifest.files) {
  const source=readFileSync(join(root,file.vendored));
  assert.equal(createHash('sha256').update(source).digest('hex'),file.sha256,`source hash ${file.path}`);
  sourceFiles.set(file.path,{...file,lines:source.toString('utf8').split('\n')});
 }
 for(const record of manifest.cases) {
  const {source}=record,file=sourceFiles.get(source.path);
  assert.ok(file,`allowlisted file ${source.path}`);
  assert.equal(source.sha256,file.sha256);
  const testLine=file.lines[source.testLine-1];
  assert.ok(testLine===`test(${JSON.stringify(source.test)}, async () => {` || testLine===`test('${source.test}', async () => {`, `test title ${source.path}:${source.testLine}`);
  assert.equal(file.lines[source.expectedLine-1].trim(),`const expected = ${record.expected.valid};`);
  assert.equal(file.lines[source.assertionLine-1].trim(),'expect(actual).toEqual(expected);');
  assert.ok(file.lines.slice(source.testLine,source.expectedLine).some(line=>line.trim()===source.call));
 }
 return manifest;
}
