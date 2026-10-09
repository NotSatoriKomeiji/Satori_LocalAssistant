import {invoke} from '@tauri-apps/api/core';
import {native,command} from './bridge';
export type Word={text:string;uses:number;last:number;pinned:boolean};
export type Monitor={id:string;name:string;value:number};
export type AssistStatus={enabled:boolean;limit:number;words:Word[];monitors:Monitor[];message:string;input_message:string;running:boolean;demo:boolean;ai:{enabled:boolean;awake:boolean;message:string}};
export type Request={op:'status'|'brightness_read'}|{op:'quick_words';enabled:boolean}|{op:'limit';limit:number}|{op:'add_word'|'delete_word';text:string}|{op:'brightness_set';device:string;before:number;value:number}|{op:'ai_mode';enabled:boolean;endpoint:string;model:string;key:string}|{op:'experience_answer';id:string;revision:number;accept:boolean}|{op:'experience_forget';id:string}|{op:'demo_correction'}|{op:'ask';prompt:string}|{op:'ai';endpoint:string;model:string;key:string;prompt:string};
const initial:AssistStatus={ai:{enabled:false,awake:false,message:'本地模式 · AI 未连接'},enabled:false,limit:50,words:[{text:'收到，我会尽快处理。',uses:2,last:0,pinned:true},{text:'谢谢你的帮助！',uses:2,last:0,pinned:true}],monitors:[{id:'demo:display',name:'模拟外接显示器',value:50}],message:'正在了解你的亮度习惯',input_message:'快速词已关闭',running:false,demo:true};
let preview:AssistStatus=structuredClone(initial);try{const stored=JSON.parse(localStorage.getItem('satori:quick-words')??'null');if(stored)preview={...initial,...stored};}catch{}
export async function assist<T=AssistStatus>(r:Request):Promise<T>{if(native)return invoke<T>('assist_api',{request:r});
 switch(r.op){
 case 'quick_words':preview.enabled=r.enabled;preview.running=r.enabled;preview.input_message=r.enabled?'已开启预览；真实输入监听需要Windows桌面版':'快速词已关闭';break;
 case 'limit':if(!Number.isInteger(r.limit)||r.limit<1||r.limit>200)throw Error('最多保存1–200条快速词');preview.limit=r.limit;break;
 case 'add_word':{const text=r.text.trim();if([...text].length<2||[...text].length>120||/[\d@]|密码|验证码|token|password|:\/\//i.test(text))throw Error('请添加2–120字的普通词句，避开号码、账号和密码');if(!preview.words.some(w=>w.text===text)){if(preview.words.length>=preview.limit)throw Error('已达到保存上限');preview.words.push({text,uses:2,last:Date.now()/1000,pinned:true});}break;}
 case 'delete_word':preview.words=preview.words.filter(w=>w.text!==r.text);break;
 case 'brightness_set':{const m=preview.monitors.find(m=>m.id===r.device);if(!m||m.value!==r.before)throw Error('亮度已经变化，请刷新');if(!Number.isInteger(r.value)||r.value<10||r.value>100)throw Error('亮度范围10–100');m.value=r.value;preview.message='已模拟调整外接屏亮度';break;}
 case 'demo_correction':await command('demo_correction');break;
 case 'experience_forget':await command('remove_experience',{id:r.id});break;
 case 'experience_answer':throw Error('网页预览不会采纳真实 AI 结果');
 case 'ai_mode':if(r.enabled)throw Error('网页预览不会连接 AI 服务');preview.ai={enabled:false,awake:false,message:'本地模式 · AI 未连接'};break;
 case 'ai':case 'ask':throw Error('网页预览不会发送AI请求');
 }
 localStorage.setItem('satori:quick-words',JSON.stringify(preview));return structuredClone({...preview,words:preview.words.slice(0,preview.limit)}) as T;
}
