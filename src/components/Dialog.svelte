<script lang="ts">
  import {dialog as promptDialog,modal} from '../lib/ui';
  let value=$state('');
  $effect(()=>{value=$promptDialog?.input??'';});
</script>
{#if $promptDialog}
  <div class="modal-shade" role="presentation">
    <dialog class="dialog" use:modal aria-label={$promptDialog.title} oncancel={e=>{e.preventDefault();$promptDialog?.resolve(null);}}><form onsubmit={e=>{e.preventDefault();$promptDialog?.resolve($promptDialog.input===undefined?'yes':value);}}>
      <h2>{$promptDialog.title}</h2><p class="prewrap">{$promptDialog.message}</p>
      {#if $promptDialog.input!==undefined}<input aria-label={$promptDialog.title} bind:value />{/if}
      <footer><button type="button" onclick={()=> $promptDialog?.resolve(null)}>Cancel</button><button class:danger={$promptDialog.danger} class="primary">{$promptDialog.confirm}</button></footer>
    </form></dialog>
  </div>
{/if}
