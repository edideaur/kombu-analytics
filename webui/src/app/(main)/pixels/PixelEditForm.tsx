import {
  Button,
  Column,
  Form,
  FormField,
  FormSubmitButton,
  Grid,
  Icon,
  Label,
  Loading,
  Row,
  TextField,
} from '@umami/react-zen';
import { useState } from 'react';
import { useConfig, useMessages, usePixelQuery } from '@/components/hooks';
import { useUpdateQuery } from '@/components/hooks/queries/useUpdateQuery';
import { RefreshCw } from '@/components/icons';
import { PIXELS_URL } from '@/lib/constants';
import { getRandomChars } from '@/lib/generate';

const generateId = () => getRandomChars(9);

export function PixelEditForm({
  pixelId,
  teamId,
  onSave,
  onClose,
}: {
  pixelId?: string;
  teamId?: string;
  onSave?: () => void;
  onClose?: () => void;
}) {
  const { t, labels, messages, getErrorMessage } = useMessages();
  const { mutateAsync, error, isPending, touch, toast } = useUpdateQuery(
    pixelId ? `/pixels/${pixelId}` : '/pixels',
    {
      id: pixelId,
      teamId,
    },
  );
  const config = useConfig();
  const pixelsUrl = config?.pixelsUrl;
  const hostUrl = pixelsUrl || PIXELS_URL;
  const { data, isLoading } = usePixelQuery(pixelId);
  const [defaultSlug] = useState(generateId());

  const handleSubmit = async (data: any) => {
    await mutateAsync(data, {
      onSuccess: async () => {
        toast(t(messages.saved));
        touch('pixels');
        touch(`pixel:${pixelId}`);
        onSave?.();
        onClose?.();
      },
    });
  };

  if (pixelId && isLoading) {
    return <Loading placement="absolute" />;
  }

  return (
    <Form
      onSubmit={handleSubmit}
      error={getErrorMessage(error)}
      defaultValues={{ slug: defaultSlug, ...data }}
    >
      {({ setValue, watch }) => {
        const slug = watch('slug');

        return (
          <>
            <FormField label={t(labels.name)} name="name" rules={{ required: t(labels.required) }}>
              <TextField autoComplete="off" autoFocus />
            </FormField>

            <Grid columns="1fr auto" alignItems="end" gap>
              <FormField
                name="slug"
                label={t(labels.slug)}
                rules={{
                  required: t(labels.required),
                }}
              >
                <TextField autoComplete="off" placeholder="e.g. tracking-image" />
              </FormField>
              <Button
                variant="quiet"
                onPress={() => setValue('slug', generateId(), { shouldDirty: true })}
              >
                <Icon>
                  <RefreshCw />
                </Icon>
              </Button>
            </Grid>

            <Column>
              <Label>{t(labels.link)}</Label>
              <Row alignItems="center" gap>
                <TextField
                  value={`${hostUrl}/${slug || ''}`}
                  autoComplete="off"
                  isReadOnly
                  allowCopy
                  style={{ width: '100%' }}
                />
              </Row>
            </Column>

            <Row justifyContent="flex-end" paddingTop="3" gap="3">
              {onClose && (
                <Button isDisabled={isPending} onPress={onClose}>
                  {t(labels.cancel)}
                </Button>
              )}
              <FormSubmitButton isDisabled={false}>{t(labels.save)}</FormSubmitButton>
            </Row>
          </>
        );
      }}
    </Form>
  );
}
