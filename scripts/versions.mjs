import {readFileSync,writeFileSync} from 'node:fs';
const json=p=>JSON.parse(readFileSync(p,'utf8'));
const pkg=json('package.json');const config=json('src-tauri/tauri.conf.json');
const cargo=readFileSync('src-tauri/Cargo.toml','utf8');
const version=cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const lock=json('package-lock.json');
const rustLock=readFileSync('src-tauri/Cargo.lock','utf8').match(/name = "ssh-virtual-desktop"\r?\nversion = "([^"]+)"/)?.[1];
const pattern=/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;
const requested=process.argv[2];
if(requested==='--set'){
 const next=process.argv[3];if(!pattern.test(next||''))throw new Error('Use a semantic version, for example 0.2.0 or 0.3.0-beta.1');
 pkg.version=next;lock.version=next;lock.packages[''].version=next;config.version=next;
 for(const [path,value] of [['package.json',pkg],['package-lock.json',lock],['src-tauri/tauri.conf.json',config]])writeFileSync(path,JSON.stringify(value,null,2)+'\n');
 writeFileSync('src-tauri/Cargo.toml',cargo.replace(/^(version\s*=\s*)"[^"]+"/m,`$1"${next}"`));
 const cl=readFileSync('src-tauri/Cargo.lock','utf8');writeFileSync('src-tauri/Cargo.lock',cl.replace(/(name = "ssh-virtual-desktop"\r?\nversion = )"[^"]+"/,`$1"${next}"`));
 console.log(`Set application and lockfile versions to ${next}`);
} else {
 if(!pattern.test(pkg.version)||[config.version,version,lock.version,lock.packages[''].version,rustLock].some(v=>v!==pkg.version))throw new Error('Application and lockfile versions differ. Run npm run version:set -- VERSION.');
 const tag=requested||process.env.RELEASE_TAG;
 if(tag&&tag!==`v${pkg.version}`)throw new Error(`Tag ${tag} does not match v${pkg.version}`);
 console.log(`Versions synchronized: ${pkg.version}${tag?` (${tag})`:''}`);
}
