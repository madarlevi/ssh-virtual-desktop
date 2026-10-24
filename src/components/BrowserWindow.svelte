<script lang="ts">
  import {onMount} from 'svelte';import {listen} from '@tauri-apps/api/event';import {rpc,errorText} from '../lib/api';import Icon from './Icon.svelte';
  let {id,active,geometry,modal,requestDownload}:{id:string;active:boolean;geometry:string;modal:boolean;requestDownload:(url:string)=>void}=$props();
  let area:HTMLDivElement;let address=$state('https://example.com');let current=$state('');let title=$state('Browser');let loading=$state(false);let ready=$state(false);let error=$state('');let alive=true;let synced=Promise.resolve();
  function sync(){if(!ready||!area)return;const r=area.getBoundingClientRect();const visible=active&&!modal&&r.width>5&&r.height>5;const args={id,x:Math.round(r.x),y:Math.round(r.y),width:Math.round(r.width),height:Math.round(r.height),visible};synced=synced.catch(()=>{}).then(()=>rpc('browser_bounds',args)).catch(e=>{if(alive)error=errorText(e);});}
  $effect(()=>{geometry;active;modal;ready;requestAnimationFrame(sync);});
  async function action(action:string,url?:string){try{await rpc('browser_action',{id,action,...(url?{url}:{})});}catch(e){error=errorText(e);}}
  function navigate(){let url=address.trim();if(!/^https?:\/\//i.test(url))url='https://'+url;address=url;void action('navigate',url);}
  onMount(()=>{
    const off:Array<()=>void>=[];alive=true;
    void (async()=>{try{
      off.push(await listen<{id:string;title:string}>('browser-title',e=>{if(e.payload.id===id)title=e.payload.title;}));
      off.push(await listen<{id:string;url:string;loading:boolean}>('browser-page',e=>{if(e.payload.id===id){current=e.payload.url;address=current;loading=e.payload.loading;}}));
      if(!alive){off.forEach(f=>f());return;}await rpc('browser_open',{id,url:address});if(!alive){await rpc('browser_action',{id,action:'close'});return;}ready=true;sync();
    }catch(e){error=errorText(e);}})();
    const observer=new ResizeObserver(sync);observer.observe(area);window.addEventListener('resize',sync);
    return ()=>{alive=false;ready=false;observer.disconnect();window.removeEventListener('resize',sync);off.forEach(f=>f());void synced.finally(()=>rpc('browser_action',{id,action:'close'})).catch(()=>{});};
  });
</script>
<div class="app-column browser-app">
  <form class="toolbar browser-toolbar" onsubmit={e=>{e.preventDefault();navigate();}}><button type="button" class="icon-button" title="Back" onclick={()=>action('back')}><Icon name="back" size={18}/></button><button type="button" class="icon-button" title="Forward" onclick={()=>action('forward')}><Icon name="forward" size={18}/></button><button type="button" class="icon-button" title={loading?'Stop':'Reload'} onclick={()=>action(loading?'stop':'reload')}><Icon name={loading?'stop':'refresh'} size={17}/></button><div class="address-field"><Icon name={address.startsWith('https:')?'lock':'browser'} size={15}/><input aria-label="Website address" bind:value={address} placeholder="Search or enter an HTTPS address"/></div><button type="button" class="icon-button" title="Download a direct URL to server" onclick={()=>requestDownload(current||address)}><Icon name="downloads" size={19}/></button></form>
  <div class="browser-title">{#if loading}<span class="spinner tiny"></span>{/if}{title}<span>LOCAL WEBVIEW2</span></div>
  {#if error}<div class="error-box">{error}</div>{/if}
  <div class="browser-native-area" bind:this={area}>{#if !active||modal}<div class="empty-state"><Icon name="browser" size={32}/><p>Browser paused behind your workspace</p><small>Focus this window to show the live page.</small></div>{:else if !ready}<div class="empty-state"><span class="spinner"></span><p>Starting Microsoft Edge WebView2…</p></div>{/if}</div>
</div>
