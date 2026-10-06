export default {
  extends: ['@commitlint/config-conventional'],
  defaultIgnores: false,
  ignores: [],
  rules: {
    'header-max-length': [2, 'always', 128],
    'body-max-line-length': [2, 'always', 128],
    'footer-max-line-length': [2, 'always', 128],
    'ascii-message': [2, 'always'],
    'message-max-length': [2, 'always', 128],
  },
  plugins: [{ rules: {
    'ascii-message': parsed => [/^[\x20-\x7E\n]*$/.test(parsed.raw), 'use English printable ASCII text only'],
    'message-max-length': (parsed, when, max) => [parsed.raw.replace(/\n+$/, '').length <= max, `entire message must be at most ${max} characters`],
  }}],
};
