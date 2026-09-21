import { buildPath } from '@/lib/url';

export interface ErrorResponse {
  error: {
    status: number;
    message: string;
    code?: string;
  };
}

export interface FetchResponse {
  ok: boolean;
  status: number;
  data?: any;
  error?: ErrorResponse;
}

export async function request(
  method: string,
  url: string,
  body?: string,
  headers: object = {},
  signal?: AbortSignal,
): Promise<FetchResponse> {
  return fetch(url, {
    method,
    cache: 'no-cache',
    headers: {
      Accept: 'application/json, text/csv, */*',
      'Content-Type': 'application/json',
      ...headers,
    },
    body,
    signal,
  }).then(async res => {
    const contentType = res.headers.get('content-type') || '';
    let data;
    if (contentType.includes('application/json')) {
      data = await res.json();
    } else {
      data = await res.text();
    }

    return {
      ok: res.ok,
      status: res.status,
      data,
    };
  });
}

export async function httpGet(
  path: string,
  params: object = {},
  headers: object = {},
  signal?: AbortSignal,
) {
  return request('GET', buildPath(path, params), undefined, headers, signal);
}

export async function httpDelete(
  path: string,
  params: object = {},
  headers: object = {},
  signal?: AbortSignal,
) {
  return request('DELETE', buildPath(path, params), undefined, headers, signal);
}

export async function httpPost(
  path: string,
  params: object = {},
  headers: object = {},
  signal?: AbortSignal,
) {
  return request('POST', path, JSON.stringify(params), headers, signal);
}

export async function httpPut(
  path: string,
  params: object = {},
  headers: object = {},
  signal?: AbortSignal,
) {
  return request('PUT', path, JSON.stringify(params), headers, signal);
}
