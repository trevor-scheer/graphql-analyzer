import { gql, useQuery, useMutation } from "@apollo/client";

// Demonstrates @client for local-only fields
const GET_SETTINGS = gql`
  query GetSettings {
    settings {
      theme
      notifications
      language
    }
    isLoggedIn @client
    cartItems @client
  }
`;

// Demonstrates @nonreactive to prevent re-renders
const GET_POST_WITH_STATIC_AUTHOR = gql`
  query GetPostWithStaticAuthor($id: ID!) {
    post(id: $id) {
      id
      title
      body
      author @nonreactive {
        id
        name
        avatar
      }
    }
  }
`;

interface SettingsData {
  settings: {
    theme: string;
    notifications: boolean;
    language: string;
  };
  isLoggedIn: boolean;
  cartItems: number;
}

export const SettingsPage: React.FC = () => {
  const { data, loading } = useQuery<SettingsData>(GET_SETTINGS);

  if (loading || !data) return null;

  return (
    <div>
      <h1>Settings</h1>
      <p>Theme: {data.settings.theme}</p>
      <p>Logged in: {data.isLoggedIn ? "Yes" : "No"}</p>
    </div>
  );
};
