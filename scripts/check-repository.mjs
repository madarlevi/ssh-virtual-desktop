import {execFileSync,spawnSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
const paths=execFileSync('git',['ls-files','-z','--cached','--others','--exclude-standard'],{encoding:'utf8'}).split('\0').filter(Boolean);
const forbidden=/(^|\/)(node_modules|target|dist|artifacts|\.build-cache|release-assets)(\/|$)|\.(?:exe|msi|pfx|p12|pem|key)$/i;
const patterns=[/-----BEGIN (?:OPENSSH |RSA |EC |DSA )?PRIVATE KEY-----/,/gh[pousr]_[A-Za-z0-9]{36,}/,/AKIA[0-9A-Z]{16}/];
for(const p of paths){
 if(forbidden.test(p))throw new Error(`Generated/sensitive file is not ignored: ${p}`);
 if(/\.(png|ico|icns)$/i.test(p))continue;
 const text=readFileSync(p,'utf8');
 if(patterns.some(r=>r.test(text)))throw new Error(`Potential secret in ${p}; review locally. Values are not printed.`);
}
for(const p of ['package-lock.json','src-tauri/Cargo.lock','LICENSE','.github/workflows/ci.yml','.github/workflows/release.yml']){
 if(spawnSync('git',['check-ignore','--quiet',p]).status===0)throw new Error(`Required source/config is ignored: ${p}`);
}
console.log(`${paths.length} candidate repository files checked; no generated installers, private keys or recognized token patterns found. No commits were made.`);
