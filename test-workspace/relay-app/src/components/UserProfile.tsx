import { graphql, useRefetchableFragment, readInlineData } from "react-relay";

// Demonstrates embedded Relay fragment with @refetchable
const UserProfileFragment = graphql`
  fragment UserProfileComponent_user on User
  @refetchable(queryName: "UserProfileComponentRefetchQuery") {
    id
    name
    username
    bio
    avatarUrl
    followers(first: 5) @connection(key: "UserProfile_followers") {
      edges {
        node {
          ...UserCard_user
        }
      }
    }
  }
`;

// Demonstrates @arguments in TypeScript
const ViewerUserProfile = graphql`
  query UserProfilePageQuery {
    viewer {
      user {
        ...UserProfile_user @arguments(showPosts: true, postCount: 3)
      }
    }
  }
`;

interface UserProfileProps {
  userId: string;
}

export const UserProfile: React.FC<UserProfileProps> = ({ userId }) => {
  return <div>User: {userId}</div>;
};
