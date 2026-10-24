<script lang="ts">
  import {rpc,errorText,type Profile,type Connection} from '../lib/api';
  import {ask,notify} from '../lib/ui';
  import Icon from './Icon.svelte';
  import {version} from '../../package.json';
  let {profiles,connected,save}:{profiles:Profile[];connected:(c:Connection)=>void;save:(p:Profile[])=>void}=$props();
  const fresh=():Profile=>({id:crypto.randomUUID(),name:'',host:'',port:22,username:'',auth:'password',keyPath:''});
  let profile=$state<Profile>(fresh()); let secret=$state(''); let passphrase=$state(''); let remember=$state(false); let busy=$state(false); let error=$state('');
  function choose(p:Profile) { profile={...p};secret='';passphrase='';error=''; }
  async function persist() { if(!profile.host || !profile.username) throw new Error('Enter a server and username.'); profile.name ||= `${profile.username}@${profile.host}`; const p=await rpc('save_profile',profile);profile=p;save([...profiles.filter(x=>x.id!==p.id),p]);return p; }
  async function connect(retry=false) {
    busy=true;error='';
    try { const p=await persist(); const c=await rpc('connect',{profile:p,secret,passphrase,remember});secret='';passphrase='';connected(c); }
    catch(e) {
      const info=e as {kind?:string;fingerprint?:string};
      if(info.kind==='hostUnknown' && info.fingerprint && !retry) {
        const yes=await ask('Trust this SSH host?',`First connection to ${profile.host}:${profile.port}. Verify this SHA-256 fingerprint with your server administrator before trusting it.\n\n${info.fingerprint}`,{confirm:'Trust and connect'});
        if(yes) { await rpc('trust_host',{fingerprint:info.fingerprint});await connect(true);return; }
        error='Connection cancelled. Host key was not trusted.';
      } else if(info.kind==='hostChanged') error=`HOST KEY CHANGED — connection blocked. Verify the server independently. Received fingerprint: ${info.fingerprint}`;
      else error=errorText(e);
    } finally {busy=false;}
  }
</script>
<main class="connection-screen">
  <aside class="connection-story">
    <div class="brand"><span class="brand-icon"><Icon name="terminal" size={27}/></span><span>SSH Virtual Desktop<small>YOUR SERVER. YOUR SPACE.</small></span></div>
    <div class="story-body"><div class="eyebrow">A LITTLE CLOSER TO YOUR SERVER</div><h1>Remote power.<br/><span>Local comfort.</span></h1><p>Your Linux server, in a desktop that feels like home. A real shell, remote files, and a browser that runs right here.</p><div class="feature-row"><Icon name="lock"/><span>Encrypted SSH connection</span></div><div class="feature-row"><Icon name="downloads"/><span>Downloads go straight to your server</span></div></div>
    <div class="story-bottom"><span class="status-dot"></span> Locally rendered · Windows desktop</div>
  </aside>
  <section class="connection-form">
    <div class="form-top"><span class="eyebrow">WORKSPACE CONNECTION</span><span class="version">v{version}</span></div>
    <h2>Connect to your server</h2><p class="muted">Pick up where your command line left off.</p>
    <form onsubmit={e=>{e.preventDefault();void connect();}}>
      <label>Saved profiles<select aria-label="Saved profiles" onchange={e=>{const p=profiles.find(p=>p.id===e.currentTarget.value);if(p)choose(p);else profile=fresh();}} value={profiles.some(p=>p.id===profile.id)?profile.id:''}><option value="">New connection</option>{#each profiles as p}<option value={p.id}>{p.name}</option>{/each}</select></label>
      <div class="field-grid"><label class="grow">Server address<input bind:value={profile.host} placeholder="192.168.1.10 or server.example.com" required autocomplete="off" disabled={busy}/></label><label class="port">Port<input type="number" min="1" max="65535" bind:value={profile.port} required disabled={busy}/></label></div>
      <label>Username<input bind:value={profile.username} placeholder="Your Linux username" required autocomplete="username" disabled={busy}/></label>
      <label>Authentication<div class="segmented"><button type="button" class:chosen={profile.auth==='password'} onclick={()=>profile.auth='password'}>Password</button><button type="button" class:chosen={profile.auth==='key'} onclick={()=>profile.auth='key'}>SSH private key</button></div></label>
      {#if profile.auth==='password'}<label>Password<input type="password" bind:value={secret} placeholder="Enter password or use saved credential" autocomplete="current-password" disabled={busy}/></label>
      {:else}<label>Private key<div class="input-action"><input bind:value={profile.keyPath} placeholder="Choose an OpenSSH private key" required/><button type="button" onclick={async()=>{try{const p=await rpc('pick_key',{});if(p)profile.keyPath=p;}catch(e){notify(e);}}}>Browse…</button></div></label><label>Passphrase <span class="muted">(optional)</span><input type="password" bind:value={passphrase} autocomplete="off"/></label>{/if}
      <label class="checkbox"><input type="checkbox" bind:checked={remember}/> Store secret in Windows Credential Manager</label>
      {#if error}<div class="error-box" role="alert">{error}</div>{/if}
      <button class="primary connect-button" disabled={busy}>{#if busy}<span class="spinner"></span> Connecting…{:else}<Icon name="link" size={18}/> Connect to workspace <span class="arrow">→</span>{/if}</button>
    </form>
    <div class="form-actions"><button class="subtle" disabled={busy} onclick={async()=>{try{await persist();notify('Profile saved.');}catch(e){notify(e);}}}>Save profile</button>{#if profiles.some(p=>p.id===profile.id)}<button class="subtle" onclick={async()=>{if(await ask('Delete profile?',profile.name,{confirm:'Delete',danger:true})){await rpc('delete_profile',{id:profile.id});save(profiles.filter(p=>p.id!==profile.id));profile=fresh();}}}>Delete profile</button>{/if}</div>
    <p class="connection-note"><Icon name="lock" size={15}/> Host keys are verified. Your server needs only SSH and SFTP.</p>
  </section>
</main>
