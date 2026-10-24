export interface BrowserTab { id:string; url:string; title:string; favicon:string; loading:boolean; ready:boolean; popupParent?:string }
export function newTab(id:string,url='',popupParent?:string):BrowserTab {return {id,url,title:'New tab',favicon:'',loading:false,ready:false,...(popupParent?{popupParent}:{})};}
export function selectRelative(ids:string[],current:string,offset:number):string {const i=ids.indexOf(current);return ids.length?ids[(Math.max(0,i)+offset+ids.length)%ids.length]:'';}
export function afterClose(ids:string[],current:string,removed:string):string {if(current!==removed)return current;const remaining=ids.filter(id=>id!==removed);return remaining[Math.min(ids.indexOf(removed),remaining.length-1)]||'';}
export function addressUrl(input:string):string {
  const value=input.trim();if(!value)return '';
  if(/^https?:\/\//i.test(value)){const u=new URL(value);if(u.username||u.password)throw new Error('Use the website sign-in page instead of credentials in its address.');return u.href;}
  if(!/\s/.test(value)&&(/^[\w-]+(\.[\w-]+)+(?:[:/]|$)/u.test(value)||/^localhost(?:[:/]|$)/i.test(value)))return new URL('https://'+value).href;
  if(/^[a-z][a-z\d+.-]*:/i.test(value))throw new Error('Only HTTP and HTTPS pages are supported.');
  return 'https://duckduckgo.com/?q='+encodeURIComponent(value);
}
export function shortcutAction(key:string,ctrl:boolean,shift:boolean,alt:boolean):string|undefined {
  if(alt&&!ctrl)return key==='ArrowLeft'?'back':key==='ArrowRight'?'forward':undefined;
  if(!ctrl||alt)return;
  if(key==='Tab')return shift?'previous':'next';
  if(/^[1-9]$/.test(key))return 'tab-'+(Number(key)-1);
  return ({t:'new',w:'close',l:'address',r:'reload',j:'downloads',f:'find',s:'save'} as Record<string,string>)[key.toLowerCase()];
}
