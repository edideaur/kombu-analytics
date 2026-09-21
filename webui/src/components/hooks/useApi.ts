import { useMutation, useQuery } from '@tanstack/react-query';
import { useCallback } from 'react';
import { getApiUrl } from '@/lib/api-url';
import { getClientAuthToken } from '@/lib/client';
import { SHARE_CONTEXT_HEADER, SHARE_TOKEN_HEADER } from '@/lib/constants';
import { type FetchResponse, httpDelete, httpGet, httpPost, httpPut } from '@/lib/fetch';
import { useApp } from '@/store/app';

async function handleResponse(res: FetchResponse): Promise<any> {
  if (!res.ok) {
    const errorData = res?.data?.error;
    const errorObj = typeof errorData === 'object' && errorData !== null ? errorData : undefined;
    const message =
      errorObj?.message ||
      (typeof errorData === 'string' ? errorData : undefined) ||
      res?.data?.message ||
      'Request failed';
    const code = errorObj?.code;
    const status = errorObj?.status || res.status;

    return Promise.reject(Object.assign(new Error(message), { code, status }));
  }
  return Promise.resolve(res.data);
}

export function useApi() {
  const shareId = useApp(state => state.share?.shareId);
  const shareToken = useApp(state => state.shareToken?.token);

  const shareHeaders =
    shareId && shareToken
      ? { [SHARE_TOKEN_HEADER]: shareToken, [SHARE_CONTEXT_HEADER]: '1' }
      : {};

  const defaultHeaders = {
    authorization: `Bearer ${getClientAuthToken()}`,
    ...shareHeaders,
  };
  const getUrl = (url: string) => {
    return getApiUrl(url);
  };

  const getHeaders = (headers: any = {}) => {
    return { ...defaultHeaders, ...headers };
  };

  return {
    get: useCallback(
      async (
        url: string,
        params: object = {},
        headersOrOptions: any = {},
        signal?: AbortSignal,
      ) => {
        const sig = signal || headersOrOptions?.signal;
        const actualHeaders = headersOrOptions?.signal
          ? { ...headersOrOptions, signal: undefined }
          : headersOrOptions;
        return httpGet(getUrl(url), params, getHeaders(actualHeaders), sig).then(handleResponse);
      },
      [httpGet],
    ),

    post: useCallback(
      async (
        url: string,
        params: object = {},
        headersOrOptions: any = {},
        signal?: AbortSignal,
      ) => {
        const sig = signal || headersOrOptions?.signal;
        const actualHeaders = headersOrOptions?.signal
          ? { ...headersOrOptions, signal: undefined }
          : headersOrOptions;
        return httpPost(getUrl(url), params, getHeaders(actualHeaders), sig).then(handleResponse);
      },
      [httpPost],
    ),

    put: useCallback(
      async (
        url: string,
        params: object = {},
        headersOrOptions: any = {},
        signal?: AbortSignal,
      ) => {
        const sig = signal || headersOrOptions?.signal;
        const actualHeaders = headersOrOptions?.signal
          ? { ...headersOrOptions, signal: undefined }
          : headersOrOptions;
        return httpPut(getUrl(url), params, getHeaders(actualHeaders), sig).then(handleResponse);
      },
      [httpPut],
    ),

    del: useCallback(
      async (
        url: string,
        params: object = {},
        headersOrOptions: any = {},
        signal?: AbortSignal,
      ) => {
        const sig = signal || headersOrOptions?.signal;
        const actualHeaders = headersOrOptions?.signal
          ? { ...headersOrOptions, signal: undefined }
          : headersOrOptions;
        return httpDelete(getUrl(url), params, getHeaders(actualHeaders), sig).then(handleResponse);
      },
      [httpDelete],
    ),
    useQuery,
    useMutation,
  };
}
