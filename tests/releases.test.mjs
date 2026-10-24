import {test} from 'node:test';import assert from 'node:assert/strict';import {spawnSync} from 'node:child_process';import {readFileSync} from 'node:fs';
test('release validation accepts the synchronized version and rejects an incorrect tag',()=>{
 const version=JSON.parse(readFileSync(new URL('../package.json',import.meta.url),'utf8')).version;
 const valid=spawnSync(process.execPath,['scripts/versions.mjs',`v${version}`],{encoding:'utf8'});assert.equal(valid.status,0,valid.stderr);
 const invalid=spawnSync(process.execPath,['scripts/versions.mjs',`v${version}.999`],{encoding:'utf8'});assert.notEqual(invalid.status,0);assert.match(invalid.stderr,/does not match/);
});
