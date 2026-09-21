import React from 'react';
import { Link as RouterLink } from 'react-router';

export default function Link({ href, to, children, ...props }: any) {
  const target = href || to || '/';
  return (
    <RouterLink to={target} {...props}>
      {children}
    </RouterLink>
  );
}
