<script context="module" lang="ts">
  import { gql } from "graphql-tag";

  export const POST_FRAGMENT = gql`
    fragment PostFields on Post {
      id
      title
      body
    }
  `;
</script>

<script lang="ts">
  import { gql } from "graphql-tag";

  const GET_POST = gql`
    query GetPost($id: ID!) {
      post(id: $id) {
        ...PostFields
        author {
          id
          name
        }
      }
    }
  `;

  export let postId: string;
</script>

<article>
  <h1>Post Detail</h1>
  <p>{postId}</p>
</article>
