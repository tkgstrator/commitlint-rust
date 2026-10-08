import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
if(Bun.version!=='1.3.6')throw new Error(`pinned Bun 1.3.6 required, got ${Bun.version}`);
const request=JSON.parse(readFileSync(0,'utf8'));
const temporary=mkdtempSync(join(tmpdir(),'commitlint-transpile-'));
try {
const result=await Bun.build({entrypoints:request.files,target:'node',external:['*'],outdir:temporary,sourcemap:'external',plugins:request.overrides? [{name:'mutation-test-source',setup(build){build.onLoad({filter:/\.test\.ts$/},args=>args.path in request.overrides? {contents:request.overrides[args.path],loader:'ts'}:undefined);}}]:[]});
if(!result.success)throw new Error(JSON.stringify(result.logs));
const outputs={};
for(const output of result.outputs)outputs[output.path.split('/').pop()]=await output.text();
process.stdout.write(JSON.stringify({bunVersion:Bun.version,outputs}));
}finally{rmSync(temporary,{recursive:true,force:true});}
