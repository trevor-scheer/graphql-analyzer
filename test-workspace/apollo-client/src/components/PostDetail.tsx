import { gql, useQuery } from "@apollo/client";

// Demonstrates @defer for incremental delivery of comments
const GET_POST_DETAIL = gql`
  query GetPostDetail($id: ID!) {
    post(id: $id) {
      id
      title
      body
      author {
        id
        name
      }
      ... @defer(label: "comments") {
        comments(first: 20) {
          edges {
            node {
              id
              text
              author {
                id
                name
              }
            }
          }
        }
      }
    }
  }
`;

// Demonstrates @unmask on fragment spread
const POST_WITH_UNMASKED = gql`
  query PostWithUnmasked($id: ID!) {
    post(id: $id) {
      ...PostSummary @unmask
      body
    }
  }
`;

interface PostData {
  post: {
    id: string;
    title: string;
    body: string;
    author: { id: string; name: string };
    comments?: {
      edges: Array<{
        node: { id: string; text: string; author: { id: string; name: string } };
      }>;
    };
  };
}

export const PostDetail: React.FC<{ postId: string }> = ({ postId }) => {
  const { data, loading } = useQuery<PostData>(GET_POST_DETAIL, {
    variables: { id: postId },
  });

  if (loading || !data) return null;

  return (
    <article>
      <h1>{data.post.title}</h1>
      <p>By {data.post.author.name}</p>
      <div>{data.post.body}</div>
    </article>
  );
};
