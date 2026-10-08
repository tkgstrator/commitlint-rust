import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname,basename} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import childProcess,{spawnSync} from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {SourceTextModule,SyntheticModule} from 'node:vm';
import {AsyncLocalStorage} from 'node:async_hooks';
import {format} from 'node:util';
import actualRules from '@commitlint/rules';
import actualParse from '@commitlint/parse';
const here=dirname(fileURLToPath(import.meta.url));
const sourceRoot=join(here,'upstream-all-v21.2.3');
const manifestSHA='07e9e3ee102c1339aa4a8aebeb6f05882c0e86f196c4a1ab6d11b05cf14a503f';
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const base64='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const asymmetric=Symbol('arrayContaining');
function snapshot(value) {
 if(value===undefined)return {$type:'undefined'};
 if(value instanceof RegExp)return {$type:'regexp',source:value.source,flags:value.flags};
 if(typeof value==='number'&&!Number.isFinite(value))return {$type:'number',value:String(value)};
 if(value&&value[asymmetric])return {$type:'arrayContaining',value:snapshot(value.value)};
 if(Array.isArray(value))return value.map(snapshot);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,snapshot(v)]));
 return value;
}
export const tagged=value=>value===undefined? {kind:'undefined'}:{kind:'json',value:snapshot(value)};
function parseMappings(map) {
 let source=0,line=0,column=0;const result=[];
 for(const row of map.mappings.split(';')) {
  let generated=0;const segments=[];
  for(const item of row.split(',')) {
   if(!item)continue;const values=[];let current=0,shift=0;
   for(const ch of item){const digit=base64.indexOf(ch);assert.ok(digit>=0);current|=(digit&31)<<shift;if(digit&32){shift+=5;}else{values.push((current&1)?-(current>>1):current>>1);current=0;shift=0;}}
   generated+=values[0];if(values.length>=4){source+=values[1];line+=values[2];column+=values[3];segments.push({generated,source,line,column});}
  }
  result.push(segments);
 }
 return result;
}
function matchEqual(actual,expected) {
 if(expected&&expected[asymmetric]) {
  assert.ok(Array.isArray(actual),'arrayContaining requires an array');
  for(const entry of expected.value)assert.ok(actual.some(v=>{try{matchEqual(v,entry);return true;}catch{return false;}}),'arrayContaining unmatched expected member');
 }else{assert.deepStrictEqual(actual,expected);}
}
export async function runUpstreamSuite({sourceOverrides={},gitExecutable='/usr/bin/git'}={}) {
 assert.equal(typeof SourceTextModule,'function','run Node with --experimental-vm-modules');
 const manifestBytes=readFileSync(join(sourceRoot,'manifest.json'));
 assert.equal(hash(manifestBytes),manifestSHA,'frozen all-rule source manifest');
 const manifest=JSON.parse(manifestBytes);
 assert.equal(process.version,manifest.execution.node,'pinned Node required');
 assert.equal(process.versions.unicode,manifest.execution.unicode,'pinned Unicode oracle required');
 const hashes=new Map();
 for(const source of manifest.files){const bytes=readFileSync(join(sourceRoot,source.path));assert.equal(hash(bytes),source.sha256,`source hash ${source.path}`);hashes.set(source.path,source.sha256);}
 for(const path of Object.keys(sourceOverrides))assert.ok(hashes.has(path)&&path.endsWith('.test.ts'),'mutation override must target a frozen test source');
 const versions={},packageGitHeads={};
 for(const [name,expected] of Object.entries(manifest.expectedVersions)) {
  const pkg=JSON.parse(readFileSync(join(here,'node_modules',name,'package.json'),'utf8'));
  assert.equal(pkg.version,expected,`pinned package ${name}`);versions[name]=pkg.version;if(pkg.gitHead)packageGitHeads[name]=pkg.gitHead;
 }
 const toolkit=JSON.parse(readFileSync(join(here,'node_modules/es-toolkit/package.json'),'utf8'));assert.equal(toolkit.version,'1.52.0');versions['es-toolkit']=toolkit.version;
 assert.deepEqual(Object.keys(actualRules).sort(),manifest.coverage.ruleNames);
 const files=manifest.files.filter(f=>f.path.endsWith('.test.ts')).map(f=>join(sourceRoot,f.path));
 const overrides=Object.fromEntries(Object.entries(sourceOverrides).map(([p,s])=>[join(sourceRoot,p),s]));
 const transpilation=spawnSync('bun',[join(here,'transpile-full-upstream.mjs')],{input:JSON.stringify({files,overrides}),encoding:'utf8',maxBuffer:32*1024*1024,timeout:60000});
 assert.equal(transpilation.status,0,`Bun transpilation failed: ${transpilation.stderr}`);
 const compiled=JSON.parse(transpilation.stdout);assert.equal(compiled.bunVersion,manifest.execution.bun);
 const maps=new Map();for(const file of files)maps.set(pathToFileURL(file).href,parseMappings(JSON.parse(compiled.outputs[basename(file).replace(/\.ts$/,'.js.map')])));
 const location=()=>{
  const stack=new Error().stack;
  for(const line of stack.split('\n')) {
   const m=line.match(/(file:\/\/[^\s()]+\.test\.ts):(\d+):(\d+)/);if(!m||!maps.has(m[1]))continue;
   const generatedLine=Number(m[2])-1,generatedColumn=Number(m[3])-1;
   const row=maps.get(m[1])[generatedLine]??[];
   const segment=row.filter(s=>s.generated<=generatedColumn).at(-1)??row[0];
   assert.ok(segment,`missing source map ${m[1]}:${m[2]}`);
   const path=fileURLToPath(m[1]).slice(sourceRoot.length+1);
   return {path,line:segment.line+1,column:segment.column+1};
  }
  throw new Error(`missing original TS location: ${stack}`);
 };
 const executedHashes=new Map(hashes);
 for(const [path,source] of Object.entries(sourceOverrides))executedHashes.set(path,hash(source));
 const tests=[],scopes=[],rows=[],current=new AsyncLocalStorage(),parsedMetadata=new WeakMap();
 const register=(title,callback,parameterRow=null)=>{assert.equal(typeof callback,'function');tests.push({title,scopes:[...scopes],parameterRows:[...rows,...(parameterRow?[parameterRow]:[])],source:location(),callback});};
 const interpolate=(title,row)=>{
  if(row&&typeof row==='object'&&!Array.isArray(row))title=title.replace(/\$([A-Za-z0-9_]+)/g,(_,key)=>String(row[key]));
  return title.includes('%')?format(title,...(Array.isArray(row)?row:[row])):title;
 };
 const test=(title,callback)=>register(title,callback);
 test.each=data=>(title,callback)=>data.forEach((row,index)=>register(interpolate(title,row),()=>callback(...(Array.isArray(row)?row:[row])),{index,row:snapshot(row)}));
 const describe=(title,callback)=>{scopes.push(title);try{callback();}finally{scopes.pop();}};
 describe.each=data=>(title,callback)=>data.forEach((row,index)=>{rows.push({index,row:snapshot(row)});try{describe(interpolate(title,row),()=>callback(...(Array.isArray(row)?row:[row])));}finally{rows.pop();}});
 const expect=actual=>new Proxy({}, {get(_target,matcher){
  if(!['toEqual','toBe','toBeTruthy','toBeFalsy','toContain'].includes(matcher))throw new Error(`unsupported upstream matcher ${String(matcher)}`);
  return expected=>{
   const active=current.getStore();assert.ok(active,'expect outside active upstream test');
   const origin=location();
   if(matcher==='toEqual')matchEqual(actual,expected);
   else if(matcher==='toBe')assert.ok(Object.is(actual,expected),'toBe: values differ');
   else if(matcher==='toBeTruthy')assert.equal(Boolean(actual),true);
   else if(matcher==='toBeFalsy')assert.equal(Boolean(actual),false);
   else {assert.ok(typeof actual==='string'||Array.isArray(actual),'toContain requires string or array');assert.ok(actual.includes(expected),'toContain: expected member absent');}
   active.assertions.push({matcher,actual:tagged(actual),expected:tagged(expected),source:origin});
  };
 }});
 expect.arrayContaining=value=>{assert.ok(Array.isArray(value));return {[asymmetric]:true,value};};
 const wrappedRules=Object.fromEntries(Object.entries(actualRules).map(([name,fn])=>[name,(...args)=>{
  const active=current.getStore();assert.ok(active,`rule ${name} called outside a test`);
  const origin=location(),parsed=args[0],metadata=parsedMetadata.get(parsed);
  const result=fn(...args);assert.ok(!result?.then,'unexpected asynchronous upstream rule; add explicit capture support');
  assert.ok(Array.isArray(result)&&typeof result[0]==='boolean',`invalid upstream result ${name}`);
  active.calls.push({rule:name,message:parsed.raw,when:tagged(args[1]),value:tagged(args[2]),argumentCount:args.length,parserPreset:'angular',parserOptions:metadata?.options??tagged(undefined),parsed:Object.fromEntries(['header','type','scope','subject','body','footer','raw','references'].map(key=>[key,parsed[key]??(key==='references'?[]:null)])),outcome:{valid:result[0],message:tagged(result[1])},source:origin});
  return result;
 }]));
 const wrappedParse=async(...args)=>{const parsed=await actualParse(...args);parsedMetadata.set(parsed,{options:tagged(args[2])});return parsed;};
 const moduleCache=new Map();
 const synthetic=(key,exports)=>{if(moduleCache.has(key))return moduleCache.get(key);const names=Object.keys(exports);const module=new SyntheticModule(names,function(){for(const name of names)this.setExport(name,exports[name]);},{identifier:key});moduleCache.set(key,module);return module;};
 const linker=async(specifier,referencing)=>{
  if(specifier==='vitest')return synthetic('capture:vitest',{test,it:test,describe,expect});
  if(specifier==='@commitlint/parse')return synthetic('capture:parse',{default:wrappedParse,parse:wrappedParse});
  if(specifier==='./index.js')return synthetic('capture:registry',{default:wrappedRules});
  if(/^\.\/[a-z-]+\.js$/.test(specifier)){
   const name=specifier.slice(2,-3);assert.ok(name in wrappedRules,`unknown original rule ${name}`);
   const exported=name.replace(/-([a-z])/g,(_,c)=>c.toUpperCase());return synthetic(`capture:rule:${name}`,{[exported]:wrappedRules[name]});
  }
  assert.ok(specifier.startsWith('node:')||['@commitlint/types','conventional-changelog-angular'].includes(specifier),`unsupported upstream import ${specifier}`);
  return synthetic(`actual:${specifier}`,await import(specifier));
 };
 const temporary=mkdtempSync(join(tmpdir(),'commitlint-full-oracle-'));
 const oldCwd=process.cwd(),envKeys=[...new Set(['HOME','XDG_CONFIG_HOME','GIT_CONFIG_GLOBAL','GIT_CONFIG_SYSTEM','GIT_CONFIG_NOSYSTEM','GIT_CONFIG_COUNT','PATH',...Object.keys(process.env).filter(key=>key.startsWith('GIT_'))])];
 const oldEnv=Object.fromEntries(envKeys.map(key=>[key,process.env[key]]));
 const originalSpawnSync=childProcess.spawnSync;
 const gitOperations=[];
 try {
  for(const key of Object.keys(process.env).filter(key=>key.startsWith('GIT_')))delete process.env[key];
  Object.assign(process.env,{HOME:temporary,XDG_CONFIG_HOME:temporary,GIT_CONFIG_GLOBAL:'/dev/null',GIT_CONFIG_SYSTEM:'/dev/null',GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_COUNT:'0',PATH:'/usr/bin:/bin'});process.chdir(temporary);
  const git=spawnSync(gitExecutable,['--version'],{encoding:'utf8',timeout:10000});assert.equal(git.status,0,`Git unavailable: ${git.stderr}`);
  // Keep original trailer rule logic; bound and validate its actual subprocess.
  childProcess.spawnSync=(command,args,options)=>{
   assert.equal(command,'git','only the original read-only Git call is allowed');
   assert.deepEqual(args,['interpret-trailers','--parse']);
   const result=originalSpawnSync(gitExecutable,args,{...options,timeout:10000,maxBuffer:1024*1024});
   assert.ifError(result.error);assert.equal(result.signal,null,'Git interrupted');assert.equal(result.status,0,'Git interpret-trailers failed');
   gitOperations.push({testId:current.getStore()?.id,args,status:result.status,stdoutBytes:result.stdout.length,stderrBytes:result.stderr.length});
   return result;
  };
  syncBuiltinESMExports();
  for(const file of files) {
   const js=compiled.outputs[basename(file).replace(/\.ts$/,'.js')];assert.equal(typeof js,'string');
   const module=new SourceTextModule(js,{identifier:pathToFileURL(file).href,initializeImportMeta(meta,module){meta.url=module.identifier;}});
   await module.link(linker);await module.evaluate({timeout:10000});
  }
  assert.equal(tests.filter(t=>!t.source.path.endsWith('/index.test.ts')).length,manifest.coverage.functionalTests,'all expanded original tests registered');
  assert.equal(tests.filter(t=>t.source.path.endsWith('/index.test.ts')).length,manifest.coverage.metaTests,'all original meta tests registered');
  const testRecords=[],cases=[];
  for(const registration of tests) {
   const record={id:`upstream-test-${String(testRecords.length+1).padStart(3,'0')}`,title:registration.title,scopes:registration.scopes,parameterRows:registration.parameterRows,source:{...registration.source,sha256:executedHashes.get(registration.source.path)},calls:[],assertions:[]};
   let timeout;try{await current.run(record,()=>Promise.race([Promise.resolve().then(registration.callback),new Promise((_,reject)=>{timeout=setTimeout(()=>reject(new Error(`unfinished upstream callback ${registration.title}`)),10000);})]));}catch(error){error.message=`${registration.source.path}: ${registration.title}: ${error.message}`;throw error;}finally{clearTimeout(timeout);}
   assert.ok(record.assertions.length>0,`upstream test has no recorded assertions: ${record.title}`);
   record.passed=true;
   for(const call of record.calls)cases.push({id:`full-upstream-${String(cases.length+1).padStart(3,'0')}`,testId:record.id,source:{...call.source,sha256:executedHashes.get(call.source.path),test:record.title,scopes:record.scopes,testLine:record.source.line,parameterRows:record.parameterRows},...Object.fromEntries(Object.entries(call).filter(([key])=>key!=='source')),assertions:record.assertions});
   testRecords.push({...record,callCount:record.calls.length,calls:undefined});
  }
  const seenRules=[...new Set(cases.map(c=>c.rule))].sort();
  assert.deepEqual(seenRules,manifest.coverage.ruleNames,'all38 rule functions exercised');
  return {schema:3,description:'Verbatim upstream test callbacks and actual assertions executed under pinned Node; observed rule outcomes are separate from asserted expectations.',upstream:manifest.upstream,versions,packageGitHeads,execution:{node:process.version,unicode:process.versions.unicode,icu:process.versions.icu,bunTranspiler:compiled.bunVersion,git:git.stdout.trim(),gitPath:gitExecutable,sourceOverrides:Object.entries(sourceOverrides).map(([path,source])=>({path,originalSHA256:hashes.get(path),executedSHA256:hash(source)})),gitConfiguration:'inherited GIT_* removed, isolated HOME/XDG/cwd, global/system disabled, empty config count; read-only interpret-trailers, 10s deadline and 1MiB output bound'},gitOperations,sourceFiles:manifest.files,license:readFileSync(join(sourceRoot,'license.md'),'utf8'),coverage:{functionalTests:testRecords.filter(t=>!t.source.path.endsWith('/index.test.ts')).length,metaTests:testRecords.filter(t=>t.source.path.endsWith('/index.test.ts')).length,ruleNames:seenRules,ruleCalls:cases.length,assertions:testRecords.reduce((sum,t)=>sum+t.assertions.length,0)},tests:testRecords,cases};
 } finally {
  childProcess.spawnSync=originalSpawnSync;syncBuiltinESMExports();
  process.chdir(oldCwd);for(const key of envKeys){if(oldEnv[key]===undefined)delete process.env[key];else process.env[key]=oldEnv[key];}rmSync(temporary,{recursive:true,force:true});
 }
}
