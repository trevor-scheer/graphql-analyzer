<script lang="ts">
import { gql } from "graphql-tag";

export const GET_USERS = gql`
  query GetUsers {
    users {
      id
      name
      email
    }
  }
`;
</script>

<template>
  <ul>
    <li v-for="user in users" :key="user.id">{{ user.name }} - {{ user.email }}</li>
  </ul>
</template>
