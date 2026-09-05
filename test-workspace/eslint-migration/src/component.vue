<script setup lang="ts">
import { gql } from "graphql-tag";

const GET_USER = gql`
  query {
    user(id: "1") {
      name
    }
  }
`;
</script>

<template>
  <div>{{ GET_USER }}</div>
</template>
