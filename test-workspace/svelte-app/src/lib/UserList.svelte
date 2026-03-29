<script lang="ts">
  import { gql } from "graphql-tag";

  const GET_USERS = gql`
    query GetUsers {
      users {
        id
        name
        email
      }
    }
  `;

  export let users: Array<{ id: string; name: string; email: string }> = [];
</script>

<ul>
  {#each users as user (user.id)}
    <li>{user.name} - {user.email}</li>
  {/each}
</ul>
