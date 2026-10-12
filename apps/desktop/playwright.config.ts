import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir:'./tests',timeout:60000,workers:1,
  use:{channel:'msedge',viewport:{width:1440,height:1050},baseURL:'http://127.0.0.1:1420',screenshot:'only-on-failure'},
  webServer:{command:'npm run dev',url:'http://127.0.0.1:1420',reuseExistingServer:false,timeout:60000},
});
