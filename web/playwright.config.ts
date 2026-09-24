import {defineConfig} from '@playwright/test';
export default defineConfig({testDir:'tests',use:{viewport:{width:1440,height:1000},launchOptions:{executablePath:process.env.OPSD_BROWSER_EXECUTABLE}},timeout:30000});
