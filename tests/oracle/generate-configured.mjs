import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdtempSync,rmSync,symlinkSync} from 'node:fs';
import {join,dirname} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
import load from '@commitlint/load';
import lint from '@commitlint/lint';
import parse from '@commitlint/parse';
import isIgnored from '@commitlint/is-ignored';
const here=dirname(fileURLToPath(import.meta.url));
assert.equal(process.version,'v26.8.2');assert.equal(process.versions.unicode,'17.0');
const inputs=[];
function add(id,message,config,{deviation=null,nativeExpected=null,category='supported'}={}){inputs.push({id,message,configText:typeof config==='string'?config:JSON.stringify(config),deviation,nativeExpected,category});}
const conventional={extends:['@commitlint/config-conventional'],defaultIgnores:false};
const angular='conventional-changelog-angular',cc='conventional-changelog-conventionalcommits';
add('preset-100-pass','fix: '+'x'.repeat(95),conventional);
add('preset-100-fail','fix: '+'x'.repeat(96),conventional);
add('preset-128-override-pass','fix: '+'x'.repeat(123),{...conventional,rules:{'header-max-length':[2,'always',128]}});
add('preset-128-override-fail','fix: '+'x'.repeat(124),{...conventional,rules:{'header-max-length':[2,'always',128]}});
add('preset-body-100','fix: a\n\n'+'x'.repeat(101),conventional);
add('preset-body-128-override','fix: a\n\n'+'x'.repeat(101),{...conventional,rules:{'body-max-line-length':[2,'always',128]}});
add('preset-override-disabled-shorthand','INVALID: '+'x'.repeat(140),{...conventional,rules:{'header-max-length':[0]}});
add('preset-override-disabled-full-tuple','INVALID: '+'x'.repeat(140),{...conventional,rules:{'header-max-length':[0,'always',{unused:true}]}});
for(const severity of [0,1,2])add(`severity-${severity}`,'invalid header',{defaultIgnores:false,rules:{'type-empty':severity===0?[0]:[severity,'never']}});
for(const literal of ['2.0','2e0','0.0','0e0'])add(`severity-number-${literal}`,'invalid header',`{"defaultIgnores":false,"rules":{"type-empty":[${literal},"never"]}}`);
add('severity-warning-and-error-order','invalid',{defaultIgnores:false,rules:{'scope-empty':[2,'never'],'subject-empty':[1,'never'],'header-max-length':[2,'always',1],'type-empty':[2,'never']}});
add('severity-reversed-error-order','invalid',{defaultIgnores:false,rules:{'type-empty':[2,'never'],'header-max-length':[2,'always',1],'subject-empty':[1,'never'],'scope-empty':[2,'never']}});
add('disabled-trailer-no-git','fix: a',{defaultIgnores:false,rules:{'trailer-exists':[0]}});
add('disabled-value-unused','fix: a',{defaultIgnores:false,rules:{'trailer-exists':[0,'always',{unused:true}]}});
for(const name of [angular,cc]) {
 add(`parser-string-${name}`,'feat!: change\n\nBREAKING CHANGE: x',{parserPreset:name,defaultIgnores:false,rules:{'type-empty':[2,'never'],'breaking-change-exclamation-mark':[2,'always']}});
 add(`parser-object-${name}`,'feat!: change',{parserPreset:{name,parserOpts:{}},defaultIgnores:false,rules:{'type-empty':[2,'never']}});
}
for(const name of [angular,cc]) {
 add(`parser-object-prefix-no-extends-${name}`,'feat!: change REF-1',{parserPreset:{name,parserOpts:{issuePrefixes:['REF-']}},defaultIgnores:false,rules:{'type-empty':[2,'never'],'references-empty':[2,'never']}});
 add(`parser-object-prefix-with-conventional-${name}`,'feat!: change REF-1',{...conventional,parserPreset:{name,parserOpts:{issuePrefixes:['REF-']}}});
}
add('parser-options-only-overrides-conventional','feat!: change REF-1',{...conventional,parserPreset:{parserOpts:{issuePrefixes:['REF-']}}});
add('parser-empty-object-overrides-conventional','feat!: change', {...conventional,parserPreset:{}});
add('parser-name-only-overrides-conventional','feat!: change', {...conventional,parserPreset:{name:cc}});
add('parser-angular-name-only-keeps-inherited-conventional','feat!: change', {...conventional,parserPreset:{name:angular}});
add('parser-object-options-only','fix: a\n# ignored\n\nbody',{parserPreset:{parserOpts:{commentChar:'#'}},defaultIgnores:false,rules:{'body-leading-blank':[1,'always']}});
add('parser-comment-signed-off','fix: a\n\nbody\n\nSigned-off-by:\n# comment',{parserPreset:{name:angular,parserOpts:{commentChar:'#'}},defaultIgnores:false,rules:{'signed-off-by':[2,'always','Signed-off-by:']}});
for(const options of [
 {issuePrefixes:['REF-']},
 {issuePrefixes:['R-','REF-']},
 {issuePrefixes:['REF-','R-']},
 {issuePrefixes:['REF-'],issuePrefixesCaseSensitive:true},
 {issuePrefixes:['ref-'],issuePrefixesCaseSensitive:false},
 {issuePrefixes:[]},
 {issuePrefixes:['REF-'],referenceActions:['addresses']},
 {issuePrefixes:['REF-'],referenceActions:[]},
 {issuePrefixes:['REF-'],commentChar:'#'},
])add(`parser-prefix-${inputs.length}`,'fix: REF-1\n\naddresses REF-2\ncloses REF-3\n# comment REF-4',{parserPreset:{name:angular,parserOpts:options},defaultIgnores:false,rules:{'references-empty':[2,'never'],'footer-leading-blank':[1,'always']}});
add('parser-prefix-case-sensitive-miss','fix: ref-1',{parserPreset:{parserOpts:{issuePrefixes:['REF-'],issuePrefixesCaseSensitive:true}},defaultIgnores:false,rules:{'references-empty':[2,'never']}});
add('unicode-configured','fix: café',{...conventional,rules:{'subject-case':[0]}});
add('complex-case-checks','fix(API|parser): change',{defaultIgnores:false,rules:{'scope-case':[2,'always',{cases:[{case:'uppercase',when:'never'}],delimiters:['|']}]}});
add('complex-scope-enum','fix(API|parser): change',{defaultIgnores:false,rules:{'scope-enum':[2,'always',{scopes:['API','parser'],delimiters:['|']}]}});
for(const message of ['', '\n', '\n\n', ' ', '\t', 'gpg: diagnostic'])add(`empty-${inputs.length}`,message,{defaultIgnores:false,rules:{'type-empty':[2,'never']}});
for(const message of ['Merge branch main','Merge pull request #1','Revert "fix: x"','fixup! fix: x','squash! fix: x','amend! fix: x','1.2.3','v1.2.3','1.2.3-alpha.1+build.2','chore(release): v1.2.3 [skip ci]','chore: 1.2.3\nbody','\n1.2.3','Automatic merge branches','Auto-merged a into b','Merged PR 3: something','Merge remote-tracking branch origin/main']) {
 add(`default-ignore-${inputs.length}`,message,{rules:{'type-empty':[2,'never']}});
 add(`ignore-disabled-${inputs.length}`,message,{defaultIgnores:false,rules:{'type-empty':[2,'never']}});
}
for(const [id,config] of [
 ['fractional-severity',{rules:{'header-max-length':[1.5,'always',1]}}],
 ['null-value',{rules:{'header-max-length':[2,'always',null]}}],
 ['null-case-value',{rules:{'subject-case':[2,'always',null]}}],
 ['unused-native-value',{rules:{'type-empty':[2,'never','unused']}}],
 ['unknown-case-target',{rules:{'subject-case':[2,'always','no-such-case']}}],
 ['plugins-empty',{plugins:[],rules:{}}],
 ['custom-parser-pattern',{parserPreset:{parserOpts:{headerPattern:'^(.*)$'}},rules:{}}],
])add(id,'fix: a',config,{category:'profile-difference',deviation:id,nativeExpected:'config-error'});
for(const message of ['', 'Merge branch main', '\n']) {
 add(`unknown-before-shortcut-${inputs.length}`,message,{rules:{'unknown-rule':[2,'always']}},{category:'profile-difference',deviation:'validate-config-before-shortcut',nativeExpected:'config-error'});
 add(`bad-severity-before-shortcut-${inputs.length}`,message,{rules:{'type-empty':[1.5,'never']}},{category:'profile-difference',deviation:'strict-integral-severity-before-shortcut',nativeExpected:'config-error'});
}
for(const [id,text] of [
 ['config-not-object','[]'],['config-null','null'],['invalid-json','{bad'],['rules-not-object','{"rules":[]}'],['rule-not-array','{"rules":{"type-empty":"never"}}'],['empty-tuple','{"rules":{"type-empty":[]}}'],['too-long-tuple','{"rules":{"type-empty":[2,"never",null,3]}}'],['severity-3','{"rules":{"type-empty":[3,"never"]}}'],['severity-text','{"rules":{"type-empty":["2","never"]}}'],['condition-invalid','{"rules":{"type-empty":[2,"banana"]}}'],['condition-null','{"rules":{"type-empty":[2,null]}}'],['missing-condition','{"rules":{"type-empty":[2]}}'],['default-ignores-not-bool','{"defaultIgnores":"yes"}'],['parser-options-not-object','{"parserPreset":{"parserOpts":[]}}'],
])add(id,'fix: a',text,{category:'invalid-configuration'});
const snapshot=value=>value instanceof RegExp?{$type:'regexp',source:value.source,flags:value.flags}:Array.isArray(value)?value.map(snapshot):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([key,v])=>[key,snapshot(v)])):value;
const temporary=mkdtempSync(join(tmpdir(),'commitlint-configured-oracle-'));
try {
 symlinkSync(join(here,'node_modules'),join(temporary,'node_modules'),'dir');
 const versions={},packageGitHeads={};for(const name of ['@commitlint/load','@commitlint/lint','@commitlint/parse','@commitlint/config-conventional','conventional-changelog-angular','conventional-changelog-conventionalcommits','conventional-commits-parser','es-toolkit']){const p=JSON.parse(readFileSync(join(here,'node_modules',name,'package.json'),'utf8'));const expected=name.startsWith('@commitlint/')?'21.2.3':{'conventional-changelog-angular':'9.4.0','conventional-changelog-conventionalcommits':'10.4.1','conventional-commits-parser':'7.1.3','es-toolkit':'1.52.0'}[name];assert.equal(p.version,expected);versions[name]=p.version;if(p.gitHead)packageGitHeads[name]=p.gitHead;}
 const cases=[];
 for(const input of inputs) {
  const configPath=join(temporary,'.commitlintrc.json');writeFileSync(configPath,input.configText);
  const normalize=error=>String(error.message).split(temporary).join('<fixture-cwd>').split(here).join('<oracle>');
  let loaded=null,loadError=null,result=null,lintError=null,parsed=null,parseError=null,ignored=false;
  try{loaded=await load({}, {file:configPath,cwd:temporary});}catch(error){loadError=normalize(error);}
  if(loaded) {
   ignored=isIgnored(input.message.trimEnd(),{defaults:loaded.defaultIgnores,ignores:loaded.ignores});
   try{result=await lint(input.message,loaded.rules,{parserOpts:loaded.parserPreset?.parserOpts,plugins:loaded.plugins,defaultIgnores:loaded.defaultIgnores,ignores:loaded.ignores});}catch(error){lintError=normalize(error);}
   try{parsed=await parse(input.message,undefined,loaded.parserPreset?.parserOpts);}catch(error){parseError=normalize(error);}
  }
  if(input.id==='config-null'){input.nativeExpected='config-error';input.deviation='strict-root-object';}
  if(input.id==='parser-options-not-object'){input.nativeExpected='config-error';input.deviation='strict-parser-options-object';}
  const nativeExpected=input.nativeExpected??(loadError?'config-error':lintError?(input.category==='invalid-configuration'?'config-error':'lint-error'):'match');
  cases.push({...input,nativeExpected,upstream:{loadError,lintError,parseError,ignored,loaded:loaded?{rules:loaded.rules,defaultIgnores:loaded.defaultIgnores??null,parserPreset:loaded.parserPreset?.name??null,parserOpts:snapshot(loaded.parserPreset?.parserOpts??null)}:null,result:result?{valid:result.valid,errors:result.errors,warnings:result.warnings,input:result.input}:null,parsed:parsed?Object.fromEntries(['header','type','scope','subject','body','footer','raw','references'].map(key=>[key,parsed[key]??(key==='references'?[]:null)])):null}});
 }
 const fixture={schema:1,description:'Actual pinned load/lint JSON-configuration differential. Native strict-profile differences are explicit; original upstream results are never replaced.',execution:{node:process.version,unicode:process.versions.unicode},versions,packageGitHeads,cases};
 writeFileSync(new URL('../../crates/commitlint-rust/tests/fixtures/configured-upstream.json',import.meta.url),JSON.stringify(fixture,null,2)+'\n');
 console.log(JSON.stringify({cases:cases.length,matches:cases.filter(c=>c.nativeExpected==='match').length,configErrors:cases.filter(c=>c.nativeExpected==='config-error').length,lintErrors:cases.filter(c=>c.nativeExpected==='lint-error').length,deviations:cases.filter(c=>c.deviation).length},null,2));
}finally{rmSync(temporary,{recursive:true,force:true});}
