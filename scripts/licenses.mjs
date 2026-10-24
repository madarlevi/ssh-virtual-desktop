// Run after cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc.
import {readFileSync,writeFileSync,readdirSync,mkdirSync,copyFileSync,existsSync,cpSync} from 'node:fs';
import {dirname,join,relative} from 'node:path';
const metadata=JSON.parse(readFileSync(process.argv[2]||'.build-cache/cargo-metadata.json','utf8').replace(/^\uFEFF/,''));
const nodes=new Set(metadata.resolve.nodes.map(n=>n.id));
const packages=metadata.packages.filter(p=>nodes.has(p.id)&&p.name!=='ssh-virtual-desktop');
const rows=[];const copied=[];
function collect(root,target,depth=0){
 if(depth>6)return;
 for(const entry of readdirSync(root,{withFileTypes:true})){
  if(entry.name.startsWith('.')||['node_modules','target'].includes(entry.name))continue;
  const source=join(root,entry.name);const dest=join(target,entry.name);
  if(entry.isDirectory())collect(source,dest,depth+1);
  else if(/^(LICENSE|LICENCE|COPYING|NOTICE|AUTHORS|COPYRIGHT)([._-].*)?$/i.test(entry.name)&&!entry.name.endsWith('.rs')){mkdirSync(dirname(dest),{recursive:true});copyFileSync(source,dest);copied.push(dest.replaceAll('\\','/'));}
 }
}
for(const p of packages){
 const target=`third-party/licenses/rust/${p.name}-${p.version}`;
 collect(dirname(p.manifest_path),target);
 rows.push(['Rust',p.name,p.version,p.license||'UNKNOWN',`https://crates.io/crates/${p.name}/${p.version}`]);
 if(p.license?.includes('MPL-2.0')){const dest=`third-party/sources/${p.name}-${p.version}`;mkdirSync(dest,{recursive:true});cpSync(dirname(p.manifest_path),dest,{recursive:true,filter:file=>!/(^|[\\/])\.[^\\/]+/.test(relative(dirname(p.manifest_path),file))});}
}
const npm=JSON.parse(readFileSync('package-lock.json','utf8'));
for(const [key,p] of Object.entries(npm.packages).filter(([k])=>k)){
 const name=key.slice(key.lastIndexOf('node_modules/')+13);rows.push(['npm',name,p.version,p.license||'UNKNOWN',`https://www.npmjs.com/package/${name}/v/${p.version}`]);
 if(existsSync(key))collect(key,`third-party/licenses/npm/${name.replaceAll('/','_')}-${p.version}`);
}
if(rows.some(r=>r[3]==='UNKNOWN'))throw new Error('Dependency license metadata is missing; review before distribution.');
const text=`# Third-party notices\n\nGenerated from the locked Windows dependency graph and npm lockfile. The application is MIT; dependencies retain their own licenses. License texts and attribution files are in [third-party/licenses](third-party/licenses/). Unmodified MPL-2.0 package sources are included in [third-party/sources](third-party/sources/) for source availability. Do not remove those notices or apply the application's MIT license to third-party code.\n\nThe identified expressions are permissive licenses or MPL-2.0 file-level licenses; no GPL/AGPL-only dependency was identified in this resolved graph. MPL-covered files remain under MPL-2.0. This inventory is not a legal opinion or a substitute for reviewing upstream notices before distribution. Some listed packages are build-time/platform-optional dependencies and do not ship as executable code.\n\nThe original app icon is maintained in src-tauri/icons/source.svg. No downloaded wallpaper, font or stock illustration is bundled. WebView2 is a separately licensed Microsoft runtime; Rust, Node.js, MSVC, WiX and NSIS are development/packaging tools, not relicensed by this project.\n\nRegenerate with the metadata command in README and \`node scripts/licenses.mjs\`.\n\n| Ecosystem | Package | Version | Declared license |\n|---|---|---|---|\n${rows.sort((a,b)=>a[1].localeCompare(b[1])).map(([e,n,v,l,u])=>`| ${e} | [${n}](${u}) | ${v} | ${l} |`).join('\n')}\n\n${copied.length} license/notice files collected.\n`;
writeFileSync('THIRD_PARTY_NOTICES.md',text);
console.log(`${packages.length} Rust and ${Object.keys(npm.packages).length-1} npm package entries; ${copied.length} notice files copied.`);
