import {writable,get} from 'svelte/store';
import {rpc,errorText,type Download} from './api';
export const downloadState=writable<{jobs:Download[];speeds:Record<string,number>;error:string;busy:boolean}>({jobs:[],speeds:{},error:'',busy:false});
export const downloadCompletion=writable<Download|null>(null);
export const running=(job:Download)=>['Starting','Waiting','Downloading'].includes(job.status);
let generation=0;let enabled=false;let timer:ReturnType<typeof setTimeout>;let inFlight:Promise<void>|undefined;
let samples=new Map<string,{bytes:number;time:number}>();let seen=new Map<string,string>();let initial=true;
export function watchDownloads(){const ticket=++generation;enabled=true;initial=true;samples=new Map();seen=new Map();downloadState.set({jobs:[],speeds:{},error:'',busy:false});downloadCompletion.set(null);void refreshDownloads();return()=>{if(generation!==ticket)return;enabled=false;generation++;clearTimeout(timer);downloadState.set({jobs:[],speeds:{},error:'',busy:false});downloadCompletion.set(null);};}
export async function refreshDownloads(force=false):Promise<void>{
  clearTimeout(timer);if(inFlight){await inFlight;if(enabled)return refreshDownloads(force);return;}
  if(!enabled)return;const ticket=generation;
  inFlight=(async()=>{downloadState.update(s=>({...s,busy:true}));try{const jobs=await rpc('download_list',{refresh:force});if(ticket!==generation)return;const time=Date.now();const speeds:Record<string,number>={};
    for(const job of jobs){const sample=samples.get(job.id);speeds[job.id]=sample&&running(job)?Math.max(0,(job.downloaded-sample.bytes)/Math.max(.001,(time-sample.time)/1000)):0;samples.set(job.id,{bytes:job.downloaded,time});if(!initial&&job.status==='Completed'&&seen.has(job.id)&&seen.get(job.id)!=='Completed')downloadCompletion.set(job);seen.set(job.id,job.status);}
    initial=false;downloadState.set({jobs,speeds,error:'',busy:false});
  }catch(e){if(ticket===generation)downloadState.update(s=>({...s,error:errorText(e),busy:false}));}finally{inFlight=undefined;if(ticket===generation&&enabled)timer=setTimeout(()=>refreshDownloads(),get(downloadState).jobs.some(running)?2000:30000);}})();await inFlight;
}
