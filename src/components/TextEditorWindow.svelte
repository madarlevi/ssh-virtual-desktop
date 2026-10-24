<script lang="ts">
  import {onMount,onDestroy} from 'svelte';import {rpc,errorText,type TextFile} from '../lib/api';import {ask,closeGuards,notify} from '../lib/ui';import Icon from './Icon.svelte';
  let {id,path,active}:{id:string;path:string;active:boolean}=$props();let file=$state<TextFile|null>(null);let text=$state('');let error=$state('');let busy=$state(false);const dirty=$derived(file!==null&&text!==file.text);
  onMount(()=>{void (async()=>{try{file=await rpc('read_text',{path});text=file.text;}catch(e){error=errorText(e);}})();closeGuards.set(id,async()=>!dirty||!!await ask('Discard unsaved changes?',`Close ${path} without saving?`,{confirm:'Discard changes',danger:true}));});
  onDestroy(()=>closeGuards.delete(id));
  async function save(){if(!file||busy)return;busy=true;error='';try{file=await rpc('save_text',{path,text,revision:file.revision});notify('Saved to server.');}catch(e){error=errorText(e);}finally{busy=false;}}
</script>
<svelte:window onkeydown={e=>{if(active&&e.ctrlKey&&e.key==='s'){e.preventDefault();void save();}}}/>
<div class="app-column editor" role="group" aria-label="Remote text editor">
  <div class="toolbar"><Icon name="file" size={17}/><span class="ellipsis">{path}{dirty?' •':''}</span><span class="spacer"></span><button class="primary" disabled={!file||busy||!dirty} onclick={save}><Icon name="save" size={16}/>{busy?'Saving…':'Save'}</button></div>
  {#if error}<div class="error-box">{error}</div>{/if}<textarea aria-label="File contents" class="editor-text" bind:value={text} disabled={!file||busy} spellcheck="false" onkeydown={e=>{if(e.key==='Tab'){e.preventDefault();const el=e.currentTarget;const pos=el.selectionStart;text=text.slice(0,pos)+'  '+text.slice(el.selectionEnd);queueMicrotask(()=>{el.selectionStart=el.selectionEnd=pos+2;});}}}></textarea><div class="statusbar">UTF-8 <span>{text.split('\n').length} lines · {dirty?'Unsaved changes':'Saved'}</span></div>
</div>
