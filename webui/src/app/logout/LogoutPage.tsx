'use client';
import { useRouter } from 'next/navigation';
import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useApi } from '@/components/hooks';
import { removeClientAuthToken } from '@/lib/client';
import { setUser } from '@/store/app';

export function LogoutPage() {
  const router = useRouter();
  const { post } = useApi();
  const queryClient = useQueryClient();

  useEffect(() => {
    async function logout() {
      try {
        await post('/auth/logout');
      } catch {
        // Ignore network errors on logout
      }

      removeClientAuthToken();
      setUser(null);
      queryClient.clear();
      window.location.href = `${process.env.basePath || ''}/login`;
    }

    removeClientAuthToken();
    setUser(null);
    queryClient.clear();
    logout();
  }, [router, post, queryClient]);

  return null;
}
