<script lang="ts">
  import DirectoryView from './DirectoryView.svelte';import Icon from './Icon.svelte';
  import {rpc,join,textExtension,type RemoteEntry} from '../lib/api';import {ask,notify} from '../lib/ui';
  let {initial,openEditor,openTerminal}:{initial:string;openEditor:(path:string)=>void;openTerminal:(path:string)=>void}=$props();
  let path=$state('');let selected=$state<RemoteEntry|null>(null);let refresh=$state(0);
  async function operation(kind:string){if(!selected)return;const src=selected;
    try{if(kind==='delete'){if(!await ask('Delete remote item?',`Permanently delete ${src.path}? Folders must be empty.`,{confirm:'Delete',danger:true}))return;await rpc('file_op',{kind,source:src.path});}
    else{const dest=await ask(kind==='rename'?'Rename remote item':`${kind==='copy'?'Copy':'Move'} on server`,kind==='rename'?'New name:':'Absolute destination path (must not exist):',{input:kind==='rename'?src.name:src.path+'.copy',confirm:kind==='copy'?'Copy on server':'Apply'});if(!dest)return;if(kind==='rename'&&(dest.includes('/')||dest==='.'||dest==='..'))throw new Error('Use a name without slashes.');await rpc('file_op',{kind,source:src.path,destination:kind==='rename'?join(path,dest):dest});}refresh++;}
    catch(e){notify(e);}}
  function open(entry:RemoteEntry){if(textExtension.test(entry.name))openEditor(entry.path);else notify('The built-in editor supports UTF-8 text files up to 2 MiB. Use a terminal for other file types.');}
</script>
<div class="app-column">
  <div class="file-actions"><button disabled={!selected||selected.isDir} onclick={()=>selected&&open(selected)}><Icon name="edit" size={16}/> Edit</button><button disabled={!selected} onclick={()=>operation('rename')}>Rename</button><button disabled={!selected} onclick={()=>operation('copy')}><Icon name="copy" size={16}/> Copy</button><button disabled={!selected} onclick={()=>operation('move')}><Icon name="move" size={16}/> Move</button><button disabled={!selected} onclick={()=>operation('delete')}><Icon name="trash" size={16}/></button><span class="spacer"></span><button onclick={()=>openTerminal(path)}><Icon name="terminal" size={16}/> Terminal here</button></div>
  <DirectoryView {initial} refreshKey={refresh} enableScan onpath={p=>{path=p;}} onselect={e=>selected=e} onopen={open}/>
</div>
