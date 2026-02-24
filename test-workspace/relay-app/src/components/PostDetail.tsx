import { graphql, useFragment, usePaginationFragment } from "react-relay";

// Demonstrates @catch for field-level error handling
const PostQuery = graphql`
  query PostDetailPageQuery($id: ID!) {
    node(id: $id) {
      ... on Post {
        ...PostDetail_post
        author {
          name @catch(to: RESULT)
          avatarUrl @catch(to: NULL)
        }
      }
    }
  }
`;

// Demonstrates @relay(plural: true) for list fragments
const UserList = graphql`
  fragment PostDetailCommenters_users on User @relay(plural: true) {
    id
    name
    avatarUrl
  }
`;

interface PostDetailProps {
  postId: string;
}

export const PostDetailPage: React.FC<PostDetailProps> = ({ postId }) => {
  return <div>Post: {postId}</div>;
};
