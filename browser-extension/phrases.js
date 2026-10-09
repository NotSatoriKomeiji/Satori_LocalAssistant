// Plain editable fields only; the desktop service owns vocabulary and learning.
export function eligible(el){
 return !!el && (el.tagName==='TEXTAREA'||el.tagName==='INPUT'&&['text',''].includes(el.type)) && !el.disabled&&!el.readOnly&&!el.closest('[data-satori-private]')&&!/pass|secret|token|card|credit|cvc|cvv|otp|verif|auth|email|phone|account|address|ssn|密码|验证|登录|支付|密钥|身份证|银行卡|手机号|邮箱/i.test([el.name,el.id,el.autocomplete,el.getAttribute('aria-label')].join(' '))&&el.autocomplete!=='off';
}
