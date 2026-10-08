import load from '@commitlint/load';
import assert from 'node:assert/strict';
import {validateUpstreamSources} from './upstream-source.mjs';
import {evaluate} from './evaluate.mjs';
import {fileURLToPath} from 'node:url';
import {dirname} from 'node:path';
import {writeFileSync,readFileSync} from 'node:fs';
const policy=dirname(fileURLToPath(import.meta.url));
const config=await load({},{file:policy+'/commitlint.config.mjs',cwd:policy});
const inputs=[];
function add(category,message){inputs.push({category,message});}
for(const type of ['build','chore','ci','docs','feat','fix','perf','refactor','revert','style','test','banana','feature','bugfix','FIX','Fix','fix_','fix1','1',''])add('types',`${type}: change behavior`);
for(const scope of ['parser','API','a/b','*','a b','a,b','a:b','a(b)c','',')','(','.','123','foo)bar','foo(bar)','scope!','[scope]'])for(const bang of ['','!'])add('scopes',`fix(${scope})${bang}: change behavior`);
for(const header of ['fix!: change behavior','fix!!: change behavior','fix(): change behavior','fix:change behavior','fix:  change behavior','fix : change behavior','fix\t: change behavior',' fix: change behavior','fix: change behavior ','fix:','fix: ','fix:  ','fix: .','fix: ...','fix: !','fix: ?','fix: 1','fix: #123','fix(scope)!: change behavior','fix(scope) !: change behavior','fix(scope): change behavior','fix(): change behavior','fix(abc)(def): change behavior','fix(abc:def): change behavior','fix: change: behavior','fix!:','fix():','fix: :','fix: -','fix: _'])add('header-layout',header);
for(const subject of ['change behavior','Change behavior','CHANGE BEHAVIOR','ChangeBehavior','changeBehavior','change Behavior','change BEHAVIOR','cHANGE BEHAVIOR','iOS support','IOS support','API support','api Support','api-support','Api-support','URLParser','urlParser','fix API','fix api','Fix API','camelCase','PascalCase','lower_case','UPPER_CASE','snake_case','kebab-case','lower.words','UPPER.WORDS',"'Quoted text'",'"Quoted Text"','`Quoted Text`','123 Upper text','1Change','(Upper text)','[Upper text]','!Upper text','_Upper text','-Upper text',':Upper text','@Upper text','a','A','x Y','X y','xY','XY','c++ support','C++ support','a.B','A.b','a/b','A/B'])add('subject-case',`fix: ${subject}`);
for(const ending of ['.','..','...','....','!','?',';',' :',':','. ','... '])add('subject-end',`fix: change behavior${ending}`);
for(const prefix of ['', '\n','\n\n',' ','\t','\r\n'])for(const suffix of ['', '\n','\n\n','\n\n\n'])add('outer-normalization',prefix+'fix: change behavior'+suffix);
for(const prefix of [' \n','\n \n','gpg: diagnostic\n',' gpg: diagnostic\n','GPG: diagnostic\n','# editor comment\n'])add('parser-prelude',prefix+'fix: change behavior');
for(const body of ['\nbody text','\n\nbody text','\n\nBody TEXT.','\n\nline one\nline two','\n\n# editor comment','\n\nBREAKING CHANGE: new behavior','\nBREAKING CHANGE: new behavior','\n\nBREAKING-CHANGE: new behavior','\n\nRefs: #123','\n\nCloses #123','\n\nCo-authored-by: Codex','\n\nCo-authored-by: Other <other@example.com>','\n\nSigned-off-by: Other <other@example.com>','\n\nCo-authored-by: Other <other@example.com>\n\nDescription.','\n\nbody.\n\nBREAKING CHANGE: new behavior','\n\n\nbody text','\n\nbody text\n\n','\n\n ','\n\n\t','\n\n\r','\n\n\0'])add('body-footer',`fix: change behavior${body}`);
for(const size of [0,1,122,123,124,125])add('whole-header-length','fix: '+'x'.repeat(size));
for(const size of [119,120,121,122])add('whole-body-length','fix: a\n\n'+'x'.repeat(size));
for(const size of [118,119,120,121])add('whole-body-terminal-lf','fix: a\n\n'+'x'.repeat(size)+'\n\n');
for(const message of ['','\n','\n\n','Merge branch main','Revert "fix: change"','fixup! fix: change','squash! fix: change','fix: 日本語','fix: café','fix: change\tbehavior','fix: change\r\n','fix: change\0','fix: change\x7f','fix: change\x1b','fix: change\n\n'+ 'x'.repeat(129),'fix: change\n\nhttps://example.com/'+ 'x'.repeat(140)])add('strict-message-policy',message);
const seen=new Set(),cases=[];
for(const {category,message} of inputs){
 if(seen.has(message))continue;seen.add(message);
 cases.push({id:`oracle-${String(cases.length+1).padStart(3,'0')}`,category,message,...await evaluate(message,config)});
}
const packages=['@commitlint/cli','@commitlint/config-conventional','@commitlint/lint','@commitlint/parse','@commitlint/rules','conventional-changelog-angular','conventional-changelog-conventionalcommits','conventional-commits-parser'];const versions=Object.fromEntries(packages.map(name=>[name,JSON.parse(readFileSync(policy+'/node_modules/'+name+'/package.json','utf8')).version]));
const sourceManifest=validateUpstreamSources(policy+'/upstream-v21.2.3');
for(const [name,version] of Object.entries(versions))assert.equal(version,sourceManifest.expectedVersions[name],`pinned package ${name}`);
const upstreamSource=sourceManifest.upstream;
const corpus={schema:2,upstreamSource,description:'Development-only oracle from actual pinned Commitlint. valid is fixed-policy validity; upstreamValid, lintError, parseError and policyException retain independent upstream evidence. Fresh gh and attribution validation are separate.',versions,policyConfig:readFileSync(policy+'/commitlint.config.mjs','utf8'),cases};
writeFileSync(policy+'/../../crates/commitlint-rust/tests/fixtures/commitlint-golden.json',JSON.stringify(corpus,null,2)+'\n');
console.log(JSON.stringify({count:cases.length,valid:cases.filter(c=>c.valid).length,invalid:cases.filter(c=>!c.valid).length,versions},null,2));
