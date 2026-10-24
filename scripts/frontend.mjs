import { spawn } from 'node:child_process';
const mode=process.argv[2]||'build';
function run(script,args){return new Promise((resolve,reject)=>{const child=spawn(process.execPath,[script,...args],{stdio:'inherit',windowsHide:true});child.on('error',reject);child.on('exit',code=>code===0?resolve():reject(new Error(`Command exited ${code}`)));});}
try {
  if(mode==='build')await run('node_modules/svelte-check/bin/svelte-check',['--tsconfig','./tsconfig.json']);
  await run('node_modules/vite/bin/vite.js',mode==='dev'?['--host','127.0.0.1']:['build']);
} catch(e){console.error(e.message);process.exitCode=1;}
