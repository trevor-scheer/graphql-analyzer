<script setup lang="ts">
import { gql } from "@apollo/client";

const GET_POST = gql`
  query GetPost($id: ID!) {
    post(id: $id) {
      id
      title
      body
      author {
        id
        name
      }
    }
  }
`;

const CREATE_POST = gql`
  mutation CreatePost($title: String!, $body: String!) {
    createPost(title: $title, body: $body) {
      id
      title
    }
  }
`;

defineProps<{ postId: string }>();
</script>

<template>
  <article>
    <h1>Post Detail</h1>
    <p>{{ postId }}</p>
  </article>
</template>
