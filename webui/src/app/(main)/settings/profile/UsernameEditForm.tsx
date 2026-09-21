import {
  Button,
  Form,
  FormButtons,
  FormField,
  FormSubmitButton,
  TextField,
} from '@umami/react-zen';
import { useLoginQuery, useMessages, useUpdateQuery } from '@/components/hooks';

export function UsernameEditForm({ onSave, onClose }: { onSave?: () => void; onClose: () => void }) {
  const { t, labels, messages, getErrorMessage } = useMessages();
  const { user, setUser } = useLoginQuery();
  const { mutateAsync, error, isPending } = useUpdateQuery(`/users/${user.id}`);

  const handleSubmit = async (data: any) => {
    await mutateAsync(
      { username: data.username },
      {
        onSuccess: async (updatedUser: any) => {
          if (updatedUser) {
            setUser({ ...user, username: updatedUser.username || data.username });
          }
          onSave?.();
          onClose();
        },
      },
    );
  };

  return (
    <Form onSubmit={handleSubmit} error={getErrorMessage(error)} defaultValues={{ username: user.username }}>
      <FormField
        label={t(labels.username)}
        name="username"
        rules={{
          required: t(labels.required),
          minLength: { value: 3, message: 'Minimum length of 3 characters' },
        }}
      >
        <TextField autoComplete="username" data-test="input-edit-username" />
      </FormField>
      <FormButtons>
        <Button onPress={onClose}>{t(labels.cancel)}</Button>
        <FormSubmitButton isDisabled={isPending}>{t(labels.save)}</FormSubmitButton>
      </FormButtons>
    </Form>
  );
}
