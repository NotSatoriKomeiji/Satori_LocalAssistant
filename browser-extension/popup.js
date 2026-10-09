const s=await chrome.storage.local.get('status');document.getElementById('status').textContent=s.status??'请运行Satori，并安装本机连接。';
