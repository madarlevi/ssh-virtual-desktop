import { writable } from 'svelte/store';
import { errorText } from './api';
export interface Dialog { title:string; message:string; input?:string; confirm:string; danger?:boolean; resolve:(v:string|null)=>void }
export const dialog = writable<Dialog|null>(null);
let chain: Promise<unknown> = Promise.resolve();
export function ask(title:string,message:string,options:{input?:string;confirm?:string;danger?:boolean}={}):Promise<string|null> {
  const run=()=>new Promise<string|null>(resolve=>dialog.set({title,message,...options,confirm:options.confirm||'Continue',resolve:v=>{dialog.set(null);resolve(v);}}));
  const next=chain.then(run); chain=next; return next;
}
export const notice = writable('');
let noticeTimer: ReturnType<typeof setTimeout>;
export function notify(value:unknown) { notice.set(errorText(value)); clearTimeout(noticeTimer); noticeTimer=setTimeout(()=>notice.set(''),9000); }
export const closeGuards = new Map<string,()=>Promise<boolean>>();
export function modal(node:HTMLDialogElement){node.showModal();return{destroy(){node.close();}};}
