import {siteOrigin} from './privacy.js';
const last=new Map();
export async function observe(details){
  if(details.frameId!==0 || details.documentLifecycle && details.documentLifecycle!=='active')return;
  const origin=siteOrigin(details.url);if(!origin)return;
  try{
    const tab=await chrome.tabs.get(details.tabId);if(!tab.active||tab.incognito)return;
    const window=await chrome.windows.get(tab.windowId);if(!window.focused||window.incognito)return;
    const now=Date.now();if(now-(last.get(origin)??0)<60000)return;
    last.set(origin,now);if(last.size>128)last.delete(last.keys().next().value);
    const result=await chrome.runtime.sendNativeMessage('io.satori.browser',{origin,at:Math.floor(now/1000)});
    await chrome.storage.local.set({status:result.accepted?'已发送站点使用节点':'Satori未运行或未开启网站观察',lastEvent:result.accepted?now:0});
  }catch{
    await chrome.storage.local.set({status:'尚未连接Satori；请安装本机连接并开启网站观察',lastEvent:0});
  }
}
chrome.webNavigation.onCommitted.addListener(observe);
// The desktop switch is authoritative. No per-site setup or separate dictionary.
chrome.runtime.onMessage?.addListener((message,sender,reply)=>{
 if(message.type!=='satori:quick_words')return;
 (async()=>{try{
  if(!sender.tab||sender.frameId!==0||sender.tab.incognito)throw Error('unsupported');
  const origin=siteOrigin(sender.url);if(!origin)throw Error('sensitive');
  const tab=await chrome.tabs.get(sender.tab.id),win=await chrome.windows.get(tab.windowId);
  if(!tab.active||tab.incognito||!win.focused||win.incognito||siteOrigin(tab.url)!==origin)throw Error('inactive');
  const text=typeof message.text==='string'&&message.text.length<=512?message.text:undefined;
  const result=await chrome.runtime.sendNativeMessage('io.satori.browser',{type:'quick_words',origin,...(text?{text}:{})});reply(result);
 }catch{reply({enabled:false,words:[]});}})();return true;
});
