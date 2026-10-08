import {readFileSync,writeFileSync} from 'node:fs';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import rules from '@commitlint/rules';
import parse from '@commitlint/parse';
import load from '@commitlint/load';
import {evaluate,parsedFields} from './evaluate.mjs';
import {validateUpstreamSources} from './upstream-source.mjs';
const here=dirname(fileURLToPath(import.meta.url));
const sourceRoot=join(here,'upstream-v21.2.3');
const manifest=validateUpstreamSources(sourceRoot);
const config=await load({}, {file:join(here,'commitlint.config.mjs'),cwd:here});
const cases=[],policyCases=[];
for(const record of manifest.cases) {
 const {source}=record;
 assert.equal(record.parserPreset,'angular');
 // Upstream parse defaults to Angular. The sole commentChar override is
 // retained explicitly, including the original raw input for blank-line rules.
 const parsed=await parse(record.message,undefined,record.parserOptions);
 const when=record.when ?? undefined,value=record.value ?? undefined;
 const outcome=await rules[record.rule](parsed,when,value);
 assert.equal(outcome[0],record.expected.valid,`${source.path}: ${source.test}`);
 const expected={valid:record.expected.valid};
 if(outcome[1]!==undefined)expected.message=outcome[1];
 const id=`upstream-rule-${String(cases.length+1).padStart(3,'0')}`;
 cases.push({...record,id,expected,parsed:parsedFields(parsed)});
 policyCases.push({id:`upstream-input-${String(policyCases.length+1).padStart(3,'0')}`,category:'upstream-test-input',provenanceKind:'upstream-input-policy-oracle',source,message:record.message,...await evaluate(record.message,config)});
}
const packages=['@commitlint/rules','@commitlint/parse','@commitlint/lint','@commitlint/config-conventional','conventional-changelog-angular','conventional-changelog-conventionalcommits','conventional-commits-parser'];
const versions={},packageGitHeads={};
for(const name of packages) {
 const pkg=JSON.parse(readFileSync(join(here,'node_modules',name,'package.json'),'utf8'));
 versions[name]=pkg.version;
 assert.equal(pkg.version,manifest.expectedVersions[name],`pinned package ${name}`);
 if(pkg.gitHead)packageGitHeads[name]=pkg.gitHead;
}
const common={upstream:manifest.upstream,versions,packageGitHeads,sourceFiles:manifest.files,license:readFileSync(join(sourceRoot,'LICENSE'),'utf8')};
const output=join(here,'../../crates/commitlint-rust/tests/fixtures');
writeFileSync(join(output,'upstream-rules.json'),JSON.stringify({schema:1,description:'Actual upstream rule assertion reuse, with reviewed argument bindings and pinned rule execution. Not whole-policy assertions.',...common,coverageNotes:['The final two footer-max-line-length test titles say multiple lines, but their callbacks repeat parsed.short and parsed.long. Preserve actual calls rather than unused message declarations.'],cases},null,2)+'\n');
writeFileSync(join(output,'upstream-input-policy.json'),JSON.stringify({schema:2,description:'Upstream raw test inputs evaluated independently through our complete pinned policy; individual upstream rule assertions are not whole-policy expectations.',...common,policyConfig:readFileSync(join(here,'commitlint.config.mjs'),'utf8'),cases:policyCases},null,2)+'\n');
console.log(JSON.stringify({ruleAssertions:cases.length,policyInputs:policyCases.length,versions},null,2));
