<script lang="ts">
  import {onMount} from 'svelte';
  import {Terminal} from '@xterm/xterm';
  import {FitAddon} from '@xterm/addon-fit';
  import '@xterm/xterm/css/xterm.css';
  import {listen} from '@tauri-apps/api/event';
  import {rpc,errorText} from '../lib/api';
  import {attachShellColors,terminalPalette} from '../lib/terminal-colors';
  import Icon from './Icon.svelte';

  let {id,cwd,active}:{id:string;cwd?:string;active:boolean}=$props();
  let host:HTMLDivElement;let term:Terminal|undefined;let fit:FitAddon|undefined;
  let error=$state('');let directory=$state('Remote shell');let integrated=$state(false);let connected=$state(false);
  $effect(()=>{if(active)requestAnimationFrame(()=>{fit?.fit();term?.focus();});});
  onMount(()=>{
    let disposed=false;let opened=false;let off:(()=>void)|undefined;let sendQueue=Promise.resolve();
    term=new Terminal({cursorBlink:true,cursorStyle:'bar',fontFamily:'Cascadia Code, Consolas, monospace',fontSize:14,lineHeight:1.25,scrollback:5000,theme:{...terminalPalette,foreground:'#f2f6fc'},allowProposedApi:true});
    fit=new FitAddon();term.loadAddon(fit);term.open(host);fit.fit();
    const colors=attachShellColors(term,path=>directory=path,()=>integrated=true);
    term.attachCustomKeyEventHandler(e=>{
      if(e.type==='keydown'&&e.ctrlKey&&e.shiftKey&&e.code==='KeyC'){void navigator.clipboard.writeText(term!.getSelection()).catch(()=>{});return false;}
      if(e.type==='keydown'&&e.ctrlKey&&e.shiftKey&&e.code==='KeyV'){void navigator.clipboard.readText().then(t=>term?.paste(t)).catch(()=>{});return false;}
      return true;
    });
    const input=term.onData(data=>{if(opened)sendQueue=sendQueue.then(async()=>{if(opened&&!disposed)await rpc('terminal_write',{id,data});}).catch(e=>{error=errorText(e);});});
    const resize=term.onResize(({cols,rows})=>{if(opened)void rpc('terminal_resize',{id,cols,rows}).catch(()=>{});});
    const observer=new ResizeObserver(()=>requestAnimationFrame(()=>{if(!disposed)fit?.fit();}));observer.observe(host);
    void (async()=>{try{
      off=await listen<{data:string|null;closed:boolean}>(`terminal-${id}`,e=>{
        if(e.payload.data){const raw=atob(e.payload.data);term?.write(Uint8Array.from(raw,c=>c.charCodeAt(0)));}
        if(e.payload.closed){opened=false;connected=false;error='SSH terminal closed. Open a new terminal after reconnecting.';}
      });
      if(disposed){off();return;}
      await rpc('terminal_open',{id,cols:term!.cols,rows:term!.rows,...(cwd?{cwd}:{})});opened=true;connected=true;
      if(disposed)await rpc('terminal_close',{id});else term?.focus();
    }catch(e){error=errorText(e);}})();
    return()=>{disposed=true;connected=false;observer.disconnect();input.dispose();resize.dispose();off?.();colors.dispose();term?.dispose();term=undefined;if(opened)void rpc('terminal_close',{id}).catch(()=>{});};
  });
</script>
<div class="terminal-app">
  <div class="terminal-session"><Icon name="terminal" size={16}/><span class="terminal-directory" title={directory}>{directory}</span><span class="spacer"></span><span class="terminal-session-badge"><span class="status-dot" class:offline={!connected}></span>{integrated?'Bash · SSH':'SSH PTY'}</span></div>
  {#if error}<div class="error-box">{error}</div>{/if}
  <div class="terminal-host" bind:this={host}></div>
  <div class="terminal-hint"><span class="terminal-color-key"><i class="path-key"></i>Folder <i class="sent-key"></i>Sent <i class="output-key"></i>Output <i class="input-key"></i>Typing</span><span>Copy Ctrl+Shift+C · Paste Ctrl+Shift+V</span></div>
</div>
