(async()=>{
 const {eligible}=await import(chrome.runtime.getURL('phrases.js'));
 if(location.protocol!=='https:'||/login|signin|oauth|auth|password|payment|checkout/i.test(location.pathname)||/^(login|accounts|passport|auth)\./i.test(location.hostname))return;
 let enabled=false,words=[],composing=false,target=null,before='',start=0,end=0,learnTimer,revision=0,inserted=false,shown=0;
 const host=document.createElement('div');host.style.cssText='position:fixed;right:16px;bottom:16px;z-index:2147483647;display:none';
 const shadow=host.attachShadow({mode:'open'}),box=document.createElement('div');box.style.cssText='font:14px sans-serif;color:#42324f;background:#faf7fff5;padding:12px;border:1px solid #e4d8f0;border-radius:12px;width:290px;max-height:300px;overflow:auto;box-shadow:0 6px 30px #30203c22';shadow.append(box);document.documentElement.append(host);
 function clear(){host.style.display='none';clearTimeout(learnTimer);revision++;}
 function safe(el){return document.hasFocus()&&!document.hidden&&eligible(el)&&!composing&&el.isConnected&&document.activeElement===el;}
 async function load(text){const version=revision;try{const s=await chrome.runtime.sendMessage({type:'satori:quick_words',...(text?{text}:{})});if(version!==revision)return;enabled=!!s?.enabled;words=s?.words??[];if(!enabled)clear();return enabled;}catch{enabled=false;clear();return false;}}
 function render(){if(!enabled||!safe(target))return;box.replaceChildren();const title=document.createElement('div');title.textContent='快速词 · 点一下输入';title.style.cssText='color:#857090;margin-bottom:8px';box.append(title);
 for(const w of words.slice(0,6)){const b=document.createElement('button');b.textContent=w.text;b.style.cssText='display:block;width:100%;text-align:left;background:white;color:#42324f;border:1px solid #eee5f3;border-radius:8px;padding:9px;margin-top:6px;white-space:pre-wrap';b.addEventListener('mousedown',e=>e.preventDefault());b.onclick=()=>insert(w.text);box.append(b);}
 if(!words.length){const p=document.createElement('p');p.textContent='正在学习，常用词句重复出现后会显示在这里。';box.append(p);}host.style.display='block';shown=Date.now();}
 async function insert(text){const el=target,expected=before,a=start,b=end;clearTimeout(learnTimer);if(!enabled||!safe(el)||el.value!==expected||el.selectionStart!==a||el.selectionEnd!==b)return clear();
 if(!await load()||!safe(el)||el.value!==expected||el.selectionStart!==a||el.selectionEnd!==b)return clear();
 const event=new InputEvent('beforeinput',{bubbles:true,cancelable:true,inputType:'insertText',data:text});if(!el.dispatchEvent(event)||!safe(el)||el.value!==expected||el.selectionStart!==a||el.selectionEnd!==b)return clear();
 inserted=true;try{el.setRangeText(text,a,b,'end');el.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));}finally{inserted=false;clear();}}
 document.addEventListener('focusin',e=>{clear();target=eligible(e.target)?e.target:null;before=target?.value??'';if(target)load();},true);
 document.addEventListener('focusout',clear,true);
 document.addEventListener('compositionstart',()=>{composing=true;clear();},true);
 document.addEventListener('compositionend',()=>{composing=false;},true);
 document.addEventListener('input',async e=>{if(inserted||!e.isTrusted||e.isComposing||composing)return;const el=e.target;if(!safe(el)||el.value.length>512)return clear();clear();target=el;before=el.value;start=el.selectionStart;end=el.selectionEnd;const version=revision;
 if(!await load()||revision!==version||!safe(el)||el.value!==before)return;render();const text=before;
 learnTimer=setTimeout(()=>{if(safe(el)&&el.value===text&&revision===version)load(text);},1500);
 },true);
 document.addEventListener('selectionchange',()=>{if(target&&(target.selectionStart!==start||target.selectionEnd!==end))clear();});
 // Deliberately no keydown interception. Enter, Backspace and IME remain with the editor.
 window.addEventListener('blur',clear);document.addEventListener('visibilitychange',clear);
 setInterval(()=>{if(host.style.display!=='none'){if(Date.now()-shown>30000||!safe(target))clear();else load();}},1000);
})();
