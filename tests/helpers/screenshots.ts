import {expect, type Page} from '@playwright/test';

export async function readyForScreenshot(page:Page) {
  await page.evaluate(()=>document.fonts.ready);
  const distinctGlyphs=await page.evaluate(()=>{
    const canvas=document.createElement('canvas');
    canvas.width=72;canvas.height=72;
    const context=canvas.getContext('2d')!;
    context.font=`48px ${getComputedStyle(document.documentElement).fontFamily}`;
    const rendered=['中','文','语'].map(character=>{
      context.clearRect(0,0,72,72);
      context.fillText(character,8,54);
      return Array.from(context.getImageData(0,0,72,72).data).join(',');
    });
    return new Set(rendered).size;
  });
  expect(distinctGlyphs,'Chinese glyphs must render distinctly; install fonts-noto-cjk on Linux').toBe(3);
}
