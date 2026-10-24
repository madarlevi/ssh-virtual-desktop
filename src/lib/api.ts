import { invoke } from '@tauri-apps/api/core';
export interface Profile { id: string; name: string; host: string; port: number; username: string; auth: 'password'|'key'; keyPath: string }
export interface Connection { home: string; profile: Profile }
export interface RemoteEntry { name: string; path: string; isDir: boolean; isLink: boolean; size: number; modified: number }
export interface Listing { path: string; entries: RemoteEntry[]; cached:boolean; updatedAt:number }
export interface DirectoryScan {enabled:boolean;scanning:boolean;root:string;depth:number;folders:number;entries:number;updatedAt:number;errors:string[];partial:boolean}
export interface TextFile { path: string; text: string; revision: string }
export interface Download { id: string; url: string; directory: string; filename: string; backend: string; started: number; status: string; downloaded: number; total: number|null; error: string|null; persistent: boolean; method:string; authenticated:boolean; headerCount:number }
export interface DownloadInput {url:string;offerId?:string;method?:string;headers?:Array<{name:string;value:string}>;body?:string;allowAuthentication?:boolean;browserRequest?:boolean;confirmDirect?:boolean}
export interface DownloadPreview {backend:string;request:{method:string;captured:boolean;headerNames:string[];cookieCount:number;authenticationDetected:boolean;bodyBytes:number;bodyUnavailable:boolean;contentDisposition:string|null;total:number|null}}
export interface Appearance { background: string; wallpaper: string; accent: string; theme: 'dark'|'light'; taskbar: string; window: string }
export const defaults: Appearance = { background:'#102333', wallpaper:'', accent:'#57b9f4', theme:'dark', taskbar:'#15202c', window:'#1b2837' };
interface Operations {
  initialize: { args: Record<string, never>; result: { profiles: Profile[]; appearance: Partial<Appearance>|null } };
  save_profile: { args: Profile; result: Profile };
  delete_profile: { args:{ id:string }; result: void };
  trust_host: { args:{ fingerprint:string }; result:void };
  connect: { args:{profile:Profile; secret:string; passphrase:string; remember:boolean};result:Connection };
  disconnect: { args:Record<string,never>;result:void };
  health: { args:Record<string,never>;result:{connected:boolean} };
  terminal_open: { args:{id:string;cols:number;rows:number;cwd?:string};result:void };
  terminal_write: { args:{id:string;data:string};result:void };
  terminal_resize: { args:{id:string;cols:number;rows:number};result:void };
  terminal_close: { args:{id:string};result:void };
  list_dir: { args:{path:string;refresh?:boolean};result:Listing };
  directory_scan_start:{args:{path:string;depth:number};result:DirectoryScan};
  directory_scan_status:{args:Record<string,never>;result:DirectoryScan};
  directory_scan_stop:{args:Record<string,never>;result:void};
  file_op: { args:{kind:string;source:string;destination?:string};result:void };
  read_text: { args:{path:string};result:TextFile };
  save_text: { args:TextFile;result:TextFile };
  download_start: { args:DownloadInput & {directory:string;filename:string};result:Download };
  download_preview:{args:DownloadInput;result:DownloadPreview};
  download_discard:{args:{id:string};result:void};
  download_list: { args:{refresh?:boolean};result:Download[] };
  download_cancel: { args:{id:string};result:void };
  browser_open: { args:{id:string;url:string};result:void };
  browser_bounds: { args:{id:string;x?:number;y?:number;width?:number;height?:number;visible:boolean};result:void };
  browser_action: { args:{id:string;action:string;url?:string};result:void };
  appearance: { args:Appearance;result:void };
  pick_key: { args:Record<string,never>;result:string|null };
  pick_wallpaper: { args:Record<string,never>;result:string|null };
}
export function rpc<K extends keyof Operations>(op: K, args: Operations[K]['args']): Promise<Operations[K]['result']> { return invoke('rpc', { op, args }); }
export function errorText(error: unknown): string { return typeof error === 'object' && error && 'message' in error ? String(error.message) : String(error); }
export function bytes(n: number): string { if (n < 1024) return `${n} B`; const i=Math.min(4, Math.floor(Math.log(n)/Math.log(1024))); return `${(n/1024**i).toFixed(i>1?1:0)} ${['B','KiB','MiB','GiB','TiB'][i]}`; }
export function parent(path: string) { return path.replace(/\/+$/,'').split('/').slice(0,-1).join('/') || '/'; }
export function join(path: string, name: string) { return `${path.replace(/\/+$/,'')}/${name}`; }
export const textExtension = /\.(txt|md|json|ya?ml|conf|ini|sh|log|toml|csv|xml|html|css|ts|js|py|rs)$/i;
