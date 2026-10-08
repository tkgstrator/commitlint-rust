import lint from '@commitlint/lint';
import parse from '@commitlint/parse';
export const parsedFields = parsed => parsed ? Object.fromEntries(['header','type','scope','subject','body','footer'].map(key => [key, parsed[key] ?? null])) : null;
// Independent operations: a diagnostic parse failure must never overwrite an
// already completed upstream lint result (notably the empty-message shortcut).
export async function evaluate(message, config) {
 let result, parsed = null, lintError = null, parseError = null;
 try {
  result = await lint(message, config.rules, {parserOpts:config.parserPreset?.parserOpts,plugins:config.plugins,defaultIgnores:false,ignores:[]});
 } catch(error) {
  lintError = error.message;
  result = {valid:false,errors:[{name:'parse-error',message:error.message}],warnings:[]};
 }
 try { parsed = await parse(message, undefined, config.parserPreset?.parserOpts); }
 catch(error) { parseError = error.message; }
 const policyException = message.trim().length === 0 && result.valid ? 'empty-message-policy' : null;
 return {valid:policyException ? false : result.valid,upstreamValid:result.valid,policyException,lintError,parseError,errors:result.errors.map(e=>e.name),warnings:result.warnings.map(e=>e.name),parsed:parsedFields(parsed)};
}
