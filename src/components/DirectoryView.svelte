<script lang="ts">
  import {untrack,onMount} from 'svelte';
  import {listen} from '@tauri-apps/api/event';
  import {rpc,errorText,bytes,parent,join,type RemoteEntry,type DirectoryScan} from '../lib/api';
  import {ask} from '../lib/ui';
  import Icon from './Icon.svelte';
  let {initial,foldersOnly=false,enableScan=false,onpath=()=>{},onselect=()=>{},onopen=()=>{},refreshKey=0}:{initial:string;foldersOnly?:boolean;enableScan?:boolean;onpath?:(p:string)=>void;onselect?:(e:RemoteEntry|null)=>void;onopen?:(e:RemoteEntry)=>void;refreshKey?:number}=$props();
  let path=$state('');let address=$state('');let entries=$state<RemoteEntry[]>([]);let selected=$state('');let error=$state('');let loading=$state(false);let history=$state<string[]>([]);let cursor=$state(-1);let sort=$state<'name'|'size'|'modified'>('name');let ascending=$state(true);let generation=0;
  const sorted=$derived([...entries].filter(e=>!foldersOnly||e.isDir||e.isLink).sort((a,b)=>Number(b.isDir)-Number(a.isDir)||(sort==='name'?a.name.localeCompare(b.name):(a[sort]-b[sort]))*(ascending?1:-1)));
  let cached=$state(false);let scanDepth=$state<number|undefined>(3);let scanBusy=$state(false);
  let scan=$state<DirectoryScan>({enabled:false,scanning:false,root:'',depth:0,folders:0,entries:0,updatedAt:0,errors:[],partial:false});
  onMount(()=>{let disposed=false;const off:Array<()=>void>=[];void(async()=>{try{
    off.push(await listen<{path:string|null}>('directory-cache-updated',e=>{if(!disposed&&path&&(e.payload.path===null||e.payload.path===path))void load(path,false);}));
    if(enableScan){off.push(await listen<DirectoryScan>('directory-scan',e=>{if(!disposed)scan=e.payload;}));const status=await rpc('directory_scan_status',{});if(!disposed)scan=status;}
    if(disposed)off.forEach(f=>f());
  }catch(e){if(!disposed)error=errorText(e);}})();return()=>{disposed=true;generation++;off.forEach(f=>f());};});
  async function startScan(){if(scanDepth===undefined||!Number.isInteger(scanDepth)||scanDepth<0||scanDepth>16){error='Enter a scan depth from 0 to 16.';return;}scanBusy=true;error='';try{scan=await rpc('directory_scan_start',{path,depth:scanDepth});}catch(e){error=errorText(e);}finally{scanBusy=false;}}
  async function stopScan(){scanBusy=true;try{await rpc('directory_scan_stop',{});}catch(e){error=errorText(e);}finally{scanBusy=false;}}
  async function load(next:string,record=true,force=false) {
    const ticket=++generation;loading=true;error='';
    try{const listing=await rpc('list_dir',{path:next,refresh:force});if(ticket!==generation)return;const previous=path===listing.path?selected:'';path=listing.path;address=path;entries=listing.entries;cached=listing.cached;const current=entries.find(e=>e.path===previous)||null;selected=current?.path||'';onselect(current);onpath(path);if(record){history=[...history.slice(0,cursor+1),path];cursor=history.length-1;}}
    catch(e){if(ticket===generation)error=errorText(e);}finally{if(ticket===generation)loading=false;}
  }
  let previousInitial:string|undefined;
  $effect(()=>{const requested=initial;refreshKey;const changed=previousInitial!==requested;previousInitial=requested;void load(changed?requested:untrack(()=>path)||requested,changed,!changed);});
  function travel(index:number){if(index>=0&&index<history.length){cursor=index;void load(history[index],false);}}
  async function mkdir(){const name=await ask('New folder','Enter a name for the remote folder.',{input:'New folder',confirm:'Create folder'});if(name){if(name.includes('/')||name==='.'||name==='..'){error='Use a folder name without slashes.';return;}try{await rpc('file_op',{kind:'mkdir',source:join(path,name)});await load(path,false,true);}catch(e){error=errorText(e);}}}
  function order(key:typeof sort){if(sort===key)ascending=!ascending;else{sort=key;ascending=true;}}
</script>
<div class="directory-view">
  <div class="toolbar directory-toolbar">
    <button class="icon-button" title="Back" disabled={cursor<=0} onclick={()=>travel(cursor-1)}><Icon name="back" size={18}/></button><button class="icon-button" title="Forward" disabled={cursor>=history.length-1} onclick={()=>travel(cursor+1)}><Icon name="forward" size={18}/></button><button class="icon-button" title="Parent folder" onclick={()=>load(parent(path))}><Icon name="up" size={18}/></button>
    <form class="path-form" onsubmit={e=>{e.preventDefault();void load(address);}}><input aria-label="Remote path" bind:value={address}/></form><button title="Reload current folder from server (bypass cache)" onclick={()=>load(path,false,true)}><Icon name="refresh" size={17}/> Reload</button><button title="New folder" onclick={mkdir}><Icon name="plus" size={16}/> Folder</button>
  </div>
  {#if enableScan}<div class="folder-scan-toolbar"><Icon name="search" size={16}/><label>Depth <input type="number" aria-label="Subfolder scan depth" min="0" max="16" step="1" bind:value={scanDepth}/></label><button disabled={scanBusy||scan.scanning||!path} onclick={startScan}>{#if scan.scanning}<span class="spinner tiny"></span>{/if}Scan &amp; cache</button>{#if scan.enabled}<button class="subtle" disabled={scanBusy} onclick={stopScan}>Stop cache</button>{/if}<span class="spacer"></span><span class="scan-mode">{scan.enabled?'Auto-update · 30s':'0 = current folder only'}</span></div>
    {#if scan.enabled}<div class="scan-status" class:partial={scan.partial} title={[scan.root,...scan.errors].join('\n')}><span>{scan.scanning?'Scanning':scan.partial?'Partial cache':'Cached'} · {scan.folders} folders · depth {scan.depth}</span><span class="ellipsis">{scan.root}</span>{#if scan.errors.length}<span>{scan.errors.length} errors</span>{/if}</div>{/if}
  {/if}
  <nav class="breadcrumbs" aria-label="Remote path"><button onclick={()=>load('/')}>/</button>{#each path.split('/').filter(Boolean) as part,i}<span>›</span><button onclick={()=>load('/'+path.split('/').filter(Boolean).slice(0,i+1).join('/'))}>{part}</button>{/each}</nav>
  {#if error}<div class="error-box" role="alert">{error}</div>{/if}
  <div class="file-table-wrap" aria-busy={loading}>
    <table class="file-table"><thead><tr><th><button onclick={()=>order('name')}>Name {sort==='name'?(ascending?'↑':'↓'):''}</button></th>{#if !foldersOnly}<th><button onclick={()=>order('size')}>Size</button></th><th>Type</th><th><button onclick={()=>order('modified')}>Modified</button></th>{/if}</tr></thead>
      <tbody>
        {#each sorted as entry}
          <tr class:selected={selected===entry.path} onclick={()=>{selected=entry.path;onselect(entry);}} ondblclick={()=>{if(entry.isDir){void load(entry.path);}else onopen(entry);}}>
            <td><button class="file-name" onclick={()=>{selected=entry.path;onselect(entry);}} ondblclick={e=>{e.stopPropagation();if(entry.isDir)void load(entry.path);else onopen(entry);}}><Icon name={entry.isDir?'files':entry.isLink?'link':'file'} size={19}/>{entry.name}</button>{#if foldersOnly}<button class="subtle enter-folder" onclick={()=>load(entry.path)}>Open →</button>{/if}</td>
            {#if !foldersOnly}<td>{entry.isDir?'—':bytes(entry.size)}</td><td>{entry.isLink?'Link':entry.isDir?'Folder':entry.name.split('.').pop()?.toUpperCase()||'File'}</td><td>{entry.modified?new Date(entry.modified*1000).toLocaleString():'—'}</td>{/if}
          </tr>
        {/each}
      </tbody>
    </table>
    {#if loading}<div class="directory-loading"><span class="spinner"></span> Reading remote directory…</div>{:else if !sorted.length}<div class="empty-state"><Icon name="files" size={35}/><p>{foldersOnly?'No subfolders here':'This folder is empty'}</p></div>{/if}
  </div>
  <div class="statusbar">{sorted.length} {foldersOnly?'folders':'items'} <span>{cached?'Cached listing':'Live SFTP'} · {path}</span></div>
</div>
