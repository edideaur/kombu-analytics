#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import https from 'https';
import zlib from 'node:zlib';
import { list } from 'tar';

const dest = path.resolve(process.cwd(), 'maxmind/extracted');

if (!fs.existsSync(dest)) {
  fs.mkdirSync(dest, { recursive: true });
}

let url = process.env.GEO_DATABASE_URL;
if (!url) {
  if (process.env.MAXMIND_LICENSE_KEY) {
    url = `https://download.maxmind.com/app/geoip_download?edition_id=GeoLite2-City&license_key=${process.env.MAXMIND_LICENSE_KEY}&suffix=tar.gz`;
  } else {
    url = 'https://raw.githubusercontent.com/GitSquared/node-geolite2-redist/master/redist/GeoLite2-City.tar.gz';
  }
}

console.log(`Downloading GeoLite2-City from: ${url.replace(/license_key=[^&]+/, 'license_key=***')}`);

function download(targetUrl) {
  https.get(targetUrl, (res) => {
    if (res.statusCode === 301 || res.statusCode === 302) {
      download(res.headers.location);
      return;
    }

    if (res.statusCode !== 200) {
      console.error(`Failed to download: HTTP ${res.statusCode}`);
      process.exit(1);
    }

    res
      .pipe(zlib.createGunzip())
      .pipe(
        list({
          filter: (entryPath) => entryPath.endsWith('.mmdb'),
          onentry: (entry) => {
            const outPath = path.join(dest, 'GeoLite2-City.mmdb');
            console.log(`Extracting: ${entry.path} -> ${outPath}`);
            entry.pipe(fs.createWriteStream(outPath));
          },
        })
      )
      .on('end', () => {
        console.log('GeoLite2 database downloaded and extracted successfully.');
      })
      .on('error', (err) => {
        console.error('Extraction error:', err);
        process.exit(1);
      });
  }).on('error', (err) => {
    console.error('Download error:', err);
    process.exit(1);
  });
}

download(url);
